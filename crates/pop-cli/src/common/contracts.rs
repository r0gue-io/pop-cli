use cliclack::{confirm, log::warning, spinner};
use pop_contracts::{does_contracts_node_exist, download_contracts_node};
use std::path::PathBuf;

/// Helper function to check if the contracts node binary exists, and if not download it.
/// returns:
/// - Some("") if the standalone binary exists
/// - Some(binary_cache_location) if the binary exists in pop's cache
/// - None if the binary does not exist
pub async fn check_contracts_node_and_prompt() -> anyhow::Result<Option<PathBuf>> {
	let mut node_path = None;

	// if the contracts node binary does not exist, prompt the user to download it
	let maybe_contract_node_path = does_contracts_node_exist(crate::cache()?);
	if maybe_contract_node_path == None {
		warning("⚠️ The substrate-contracts-node binary is not found.")?;
		if confirm("📦 Would you like to source it automatically now?")
			.initial_value(true)
			.interact()?
		{
			let spinner = spinner();
			spinner.start("📦 Sourcing substrate-contracts-node...");

			let cache_path = crate::cache()?;
			let binary = download_contracts_node(cache_path.clone()).await?;

			spinner.stop(format!(
				"✅  substrate-contracts-node successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			node_path = Some(binary.path());
		}
	} else {
		if let Some(contract_node_path) = maybe_contract_node_path {
			// If the node_path is not empty (cached binary). Otherwise, the standalone binary will be used by cargo-contract.
			node_path = Some(contract_node_path.0);
		}
	}

	Ok(node_path)
}
