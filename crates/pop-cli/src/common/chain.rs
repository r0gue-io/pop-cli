// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::*,
	common::rpc::{RPCNode, prompt_to_select_chain_rpc},
};
use anyhow::{Result, anyhow};
use pop_chains::{OnlineClient, Pallet, SubstrateConfig, parse_chain_metadata, set_up_client};
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
	select_message: &str,
	input_message: &str,
	default_input: &str,
	url: &Option<Url>,
	filter_fn: fn(&RPCNode) -> bool,
	cli: &mut impl Cli,
) -> Result<Chain> {
	// Resolve url.
	let url = match url {
		Some(url) => url.clone(),
		None =>
			prompt_to_select_chain_rpc(select_message, input_message, default_input, filter_fn, cli)
				.await?,
	};
	let client = set_up_client(url.as_str()).await?;
	let pallets = get_pallets(&client).await?;
	Ok(Chain { url, client, pallets })
}

// Get available pallets on the chain.
pub(crate) async fn get_pallets(client: &OnlineClient<SubstrateConfig>) -> Result<Vec<Pallet>> {
	// Parse metadata from chain url.
	let mut pallets = parse_chain_metadata(client)
		.map_err(|e| anyhow!(format!("Unable to fetch the chain metadata: {e}")))?;
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
		let select_message = "Select a chain (type to filter)";
		let input_message = "Enter the URL of the chain:";
		let mut cli = MockCli::new()
			.expect_select(
				select_message.to_string(),
				Some(true),
				true,
				Some(vec![
					("Local".to_string(), "Local node (ws://localhost:9944)".to_string()),
					("Custom".to_string(), "Type the chain URL manually".to_string()),
				]),
				1,
				None,
			)
			.expect_input(input_message, node.ws_url().into());
		let chain =
			configure(select_message, input_message, node.ws_url(), &None, |_| true, &mut cli)
				.await?;
		assert_eq!(chain.url, Url::parse(node.ws_url())?);
		// Get pallets
		let pallets = get_pallets(&chain.client).await?;
		assert!(!pallets.is_empty());

		cli.verify()
	}
}
