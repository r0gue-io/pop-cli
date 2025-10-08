// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::*,
	commands::call::chain::{RPCNode, extract_chain_endpoints},
};
use anyhow::{Result, anyhow};
use pop_chains::{OnlineClient, Pallet, SubstrateConfig, parse_chain_metadata, set_up_client};
use std::time::{SystemTime, UNIX_EPOCH};
use url::Url;

// Represents a chain and its associated metadata.
pub(crate) struct Chain {
	// Websocket endpoint of the node.
	pub url: Url,
	// The client used to interact with the chain.
	pub client: OnlineClient<SubstrateConfig>,
	// A list of pallets available on the chain.
	pub pallets: Vec<Pallet>,
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
}
