use cliclack::{
	confirm,
	log::{self, warning},
	spinner,
};
use pop_contracts::{contracts_node_generator, standalone_binary_exists};
use std::path::PathBuf;

/// Helper function to check if the contracts node binary exists, and if not download it.
/// returns:
/// - Some("") if the standalone binary exists
/// - Some(binary_cache_location) if the binary exists in pop's cache
/// - None if the binary does not exist
pub async fn check_contracts_node_and_prompt(skip_confirm: bool) -> anyhow::Result<PathBuf> {
	// default to standalone binary, if it exists.
	let mut node_path = PathBuf::from("substrate-contracts-node");

	let standalone_binary = standalone_binary_exists();
	// if the contracts node binary does not exist, prompt the user to download it
	if standalone_binary == None {
		let cache_path: PathBuf = crate::cache()?;
		let mut binary = contracts_node_generator(cache_path, None).await?;
		let mut latest = false;
		if !binary.exists() {
			warning("The substrate-contracts-node binary is not found.")?;
			if confirm("Would you like to source the substrate-contracts-node binary?")
				.initial_value(true)
				.interact()?
			{
				let spinner = spinner();
				spinner.start("Sourcing substrate-contracts-node...");

				binary.source(false, &(), true).await?;

				spinner.stop(format!(
					"substrate-contracts-node successfully sourced. Cached at: {}",
					binary.path().to_str().unwrap()
				));
				node_path = binary.path();
			}
		}
		if binary.stale() {
			log::warning(format!(
				"ℹ️ There is a newer version available:\n   {} {} -> {}",
				binary.name(),
				binary.version().unwrap_or("None"),
				binary.latest().unwrap_or("None")
			))?;
			if !skip_confirm {
				latest = confirm(
					"📦 Would you like to source it automatically now? It may take some time..."
						.to_string(),
				)
				.initial_value(true)
				.interact()?;
			} else {
				latest = true;
			}
			if latest {
				let spinner = spinner();
				spinner.start("Sourcing substrate-contracts-node...");

				binary.use_latest();
				binary.source(false, &(), true).await?;

				spinner.stop(format!(
					"substrate-contracts-node successfully sourced. Cached at: {}",
					binary.path().to_str().unwrap()
				));
				node_path = binary.path();
			}
		}
	}

	Ok(node_path)
}
