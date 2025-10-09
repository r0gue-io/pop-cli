// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use anyhow::{Result, anyhow};
use pop_chains::{OnlineClient, Pallet, SubstrateConfig, parse_chain_metadata, set_up_client};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

const CHAIN_ENDPOINTS_URL: &str =
	"https://raw.githubusercontent.com/r0gue-io/polkadot-chains/refs/heads/master/endpoints.json";

// Represents a chain and its associated metadata.
pub(crate) struct Chain {
	// Websocket endpoint of the node.
	pub url: Url,
	// The client used to interact with the chain.
	pub client: OnlineClient<SubstrateConfig>,
	// A list of pallets available on the chain.
	pub pallets: Vec<Pallet>,
}

/// Represents a node in the network with its RPC endpoints and chain properties.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RPCNode {
	/// Name of the chain (e.g. "Polkadot Relay", "Kusama Relay").
	pub name: String,
	/// List of RPC endpoint URLs that can be used to connect to this chain.
	pub providers: Vec<String>,
	/// Indicates if this chain is a relay chain.
	pub is_relay: bool,
	/// For parachains, contains the name of their relay chain. None for relay chains or
	/// solochains.
	pub relay: Option<String>,
	/// Indicates if this chain supports smart contracts. Particularly, whether pallet-revive is
	/// present in the runtime or not.
	pub supports_contracts: bool,
}

// Get the RPC endpoints from the maintained source.
pub(crate) async fn extract_chain_endpoints() -> Result<Vec<RPCNode>> {
	extract_chain_endpoints_from_url(CHAIN_ENDPOINTS_URL).await
}

// Internal function that accepts a URL parameter, making it testable with mockito.
async fn extract_chain_endpoints_from_url(url: &str) -> Result<Vec<RPCNode>> {
	let response = reqwest::get(url).await?;
	response.json().await.map_err(|e| anyhow!(e.to_string()))
}

// Configures a chain by resolving the URL and fetching its metadata.
pub(crate) async fn configure(
	input_message: &str,
	default_input: &str,
	url: &Option<Url>,
	filter_fn: fn(&RPCNode) -> bool,
	cli: &mut impl Cli,
) -> Result<Chain> {
	// Resolve url.
	let url = match url {
		Some(url) => url.clone(),
		None => {
			// Ask the user if they want to enter URL manually or select from a list of well-known
			// endpoints.
			let manual = cli
				.confirm("Do you want to enter the node URL manually?")
				.initial_value(false)
				.interact()?;
			let url = if manual {
				// Prompt for manual URL input
				cli.input(input_message).default_input(default_input).interact()?
			} else {
				// Select from available endpoints
				let chains = extract_chain_endpoints().await?;
				let mut prompt = cli.select("Select a chain (type to filter):");
				for (pos, node) in chains.iter().enumerate() {
					if filter_fn(node) {
						prompt = prompt.item(pos, &node.name, "");
					}
				}
				let selected = prompt.filter_mode().interact()?;
				let providers = &chains[selected].providers;
				let random_position = (SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
					as usize) % providers.len();
				providers[random_position].clone()
			};
			Url::parse(&url)?
		},
	};
	let client = set_up_client(url.as_str()).await?;
	let pallets = get_pallets(&client).await?;
	Ok(Chain { url, client, pallets })
}

// Get available pallets on the chain.
pub(crate) async fn get_pallets(client: &OnlineClient<SubstrateConfig>) -> Result<Vec<Pallet>> {
	// Parse metadata from chain url.
	let mut pallets = parse_chain_metadata(client)
		.map_err(|e| anyhow!(format!("Unable to fetch the chain metadata: {}", e.to_string())))?;
	// Sort by name for display.
	pallets.sort_by_key(|pallet| pallet.name.clone());
	pallets.iter_mut().for_each(|p| p.functions.sort_by_key(|f| f.name.clone()));
	Ok(pallets)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use pop_common::test_env::TestNode;

	#[tokio::test]
	async fn configure_works() -> Result<()> {
		let node = TestNode::spawn().await?;
		let message = "Enter the URL of the chain:";
		let mut cli = MockCli::new()
			.expect_confirm("Do you want to enter the node URL manually?", true)
			.expect_input(message, node.ws_url().into());
		let chain = configure(message, node.ws_url(), &None, |_| true, &mut cli).await?;
		assert_eq!(chain.url, Url::parse(node.ws_url())?);
		// Get pallets
		let pallets = get_pallets(&chain.client).await?;
		assert!(!pallets.is_empty());

		cli.verify()
	}

	#[tokio::test]
	async fn extract_chain_endpoints_works() -> Result<()> {
		// Create a mock server
		let mut server = mockito::Server::new_async().await;

		// Create mock response data
		let mock_response = serde_json::json!([
			{
				"name": "Polkadot Relay",
				"providers": [
					"wss://polkadot.api.onfinality.io/public-ws",
					"wss://rpc.polkadot.io"
				],
				"isRelay": true,
				"supportsContracts": false,
			},
			{
				"name": "Kusama Relay",
				"providers": [
					"wss://kusama.api.onfinality.io/public-ws",
				],
				"isRelay": true,
				"supportsContracts": false
			},
			{
				"name": "Asset Hub - Polkadot Relay",
				"providers": [
					"wss://polkadot-asset-hub-rpc.polkadot.io",
				],
				"isRelay": false,
				"relay": "Polkadot Relay",
				"supportsContracts": true,
			}
		]);

		// Set up the mock endpoint
		let mock = server
			.mock("GET", "/")
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(mock_response.to_string())
			.create_async()
			.await;

		// Call the function with the mock server URL
		let result = extract_chain_endpoints_from_url(&server.url()).await?;

		// Verify the mock was called
		mock.assert_async().await;

		// Verify the parsed results
		assert_eq!(result.len(), 3);

		let polkadot = result.iter().find(|n| n.name == "Polkadot Relay").unwrap();
		assert_eq!(polkadot.providers.len(), 2);
		assert!(polkadot.is_relay);
		assert_eq!(polkadot.relay, None);
		assert!(!polkadot.supports_contracts);

		let kusama = result.iter().find(|n| n.name == "Kusama Relay").unwrap();
		assert_eq!(kusama.providers.len(), 1);
		assert!(kusama.is_relay);
		assert!(!kusama.supports_contracts);

		let asset_hub = result.iter().find(|n| n.name == "Asset Hub - Polkadot Relay").unwrap();
		assert_eq!(asset_hub.providers.len(), 1);
		assert!(!asset_hub.is_relay);
		assert_eq!(asset_hub.relay, Some("Polkadot Relay".to_string()));
		assert!(asset_hub.supports_contracts);

		Ok(())
	}

	#[tokio::test]
	async fn extract_chain_endpoints_handles_missing_providers() -> Result<()> {
		let mut server = mockito::Server::new_async().await;

		// Mock response with missing providers field
		let mock_response = serde_json::json!({
			"invalid-chain": {
				"isRelay": false
			}
		});

		server
			.mock("GET", "/")
			.with_status(200)
			.with_header("content-type", "application/json")
			.with_body(mock_response.to_string())
			.create_async()
			.await;

		// Should return an error for missing providers
		let result = extract_chain_endpoints_from_url(&server.url()).await;
		assert!(result.is_err());

		Ok(())
	}
}
