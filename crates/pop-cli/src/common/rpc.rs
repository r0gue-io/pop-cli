// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::{Cli, Input, Select},
	common::urls,
};
use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

#[cfg(not(test))]
const CHAIN_ENDPOINTS_URL: &str =
	"https://raw.githubusercontent.com/r0gue-io/polkadot-chains/refs/heads/master/endpoints.json";

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

// Internal function that accepts a URL parameter, making it testable with mockito.
async fn extract_chain_endpoints_from_url(url: &str) -> Result<Vec<RPCNode>> {
	let response = reqwest::get(url).await?;
	response.json().await.map_err(|e| anyhow!(e.to_string()))
}

// Get the RPC endpoints from the maintained source.
#[cfg(not(test))]
pub(crate) async fn extract_chain_endpoints() -> Result<Vec<RPCNode>> {
	extract_chain_endpoints_from_url(CHAIN_ENDPOINTS_URL).await
}

// Do not fetch the RPC endpoints from the maintained source. Used for testing.
#[cfg(test)]
pub(crate) async fn extract_chain_endpoints() -> Result<Vec<RPCNode>> {
	Ok(Vec::new())
}

// Prompts the user to select an RPC endpoint from a list of available chains or enter a custom URL.
#[allow(unused)]
pub(crate) async fn prompt_to_select_chain_rpc(
	select_message: &str,
	input_message: &str,
	default_input: &str,
	filter_fn: fn(&RPCNode) -> bool,
	cli: &mut impl Cli,
) -> Result<Url> {
	// Select from available endpoints
	let mut prompt = cli.select(select_message);
	prompt = prompt.item(0, "Local", "Deploy on a local node");
	prompt = prompt.item(1, "Custom", "Type the chain URL manually");
	let chains = extract_chain_endpoints().await.unwrap_or_default();
	let prompt = chains.iter().enumerate().fold(prompt, |acc, (pos, node)| {
		if filter_fn(node) { acc.item(pos + 2, &node.name, "") } else { acc }
	});

	let selected = prompt.filter_mode().interact()?;
	let url = match selected {
		// Select the local URL
		0 => urls::LOCAL.to_string(),
		// Manually enter the URL
		1 => cli.input(input_message).default_input(default_input).interact()?,
		_ => {
			// Randomly select a provider from the chain's provider list
			let providers = &chains[selected - 2].providers;
			let random_position = (SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
				as usize) % providers.len();
			providers[random_position].clone()
		},
	};
	Ok(Url::parse(&url)?)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

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
