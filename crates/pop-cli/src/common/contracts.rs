// SPDX-License-Identifier: GPL-3.0

use cliclack::{confirm, log::warning, spinner};
use pop_contracts::contracts_node_generator;
use std::path::PathBuf;

///  Checks the status of the `substrate-contracts-node` binary, sources it if necessary, and
/// prompts the user to update it if the existing binary is not the latest version.
///
/// # Arguments
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_contracts_node_and_prompt(skip_confirm: bool) -> anyhow::Result<PathBuf> {
	let cache_path: PathBuf = crate::cache()?;
	let mut binary = contracts_node_generator(cache_path, None).await?;
	let mut node_path = binary.path();
	if !binary.exists() {
		warning("⚠️ The substrate-contracts-node binary is not found.")?;
		if confirm("📦 Would you like to source it automatically now?")
			.initial_value(true)
			.interact()?
		{
			let spinner = spinner();
			spinner.start("📦 Sourcing substrate-contracts-node...");

			binary.source(false, &(), true).await?;

			spinner.stop(format!(
				"✅ substrate-contracts-node successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			node_path = binary.path();
		}
	}
	if binary.stale() {
		warning(format!(
			"ℹ️ There is a newer version of {} available:\n {} -> {}",
			binary.name(),
			binary.version().unwrap_or("None"),
			binary.latest().unwrap_or("None")
		))?;
		let latest = if !skip_confirm {
			confirm(
				"📦 Would you like to source it automatically now? It may take some time..."
					.to_string(),
			)
			.initial_value(true)
			.interact()?
		} else {
			true
		};
		if latest {
			let spinner = spinner();
			spinner.start("📦 Sourcing substrate-contracts-node...");

			binary.use_latest();
			binary.source(false, &(), true).await?;

			spinner.stop(format!(
				"✅ substrate-contracts-node successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			node_path = binary.path();
		}
	}

	Ok(node_path)
}
