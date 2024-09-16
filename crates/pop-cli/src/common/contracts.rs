// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use cliclack::spinner;
use pop_contracts::contracts_node_generator;
use std::path::{Path, PathBuf};

///  Checks the status of the `substrate-contracts-node` binary, sources it if necessary, and
/// prompts the user to update it if the existing binary is not the latest version.
///
/// # Arguments
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_contracts_node_and_prompt(
	cli: &mut impl cli::traits::Cli,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	let mut binary = contracts_node_generator(cache_path, None).await?;
	let mut node_path = binary.path();
	if !binary.exists() {
		cli.warning("⚠️ The substrate-contracts-node binary is not found.")?;
		if cli
			.confirm("📦 Would you like to source it automatically now?")
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
		cli.warning(format!(
			"ℹ️ There is a newer version of {} available:\n {} -> {}",
			binary.name(),
			binary.version().unwrap_or("None"),
			binary.latest().unwrap_or("None")
		))?;
		let latest;
		if !skip_confirm {
			latest = cli
				.confirm(
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;

	#[tokio::test]
	async fn check_contracts_node_and_prompt_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let mut cli = MockCli::new()
			.expect_warning("⚠️ The substrate-contracts-node binary is not found.")
			.expect_confirm("📦 Would you like to source it automatically now?", true)
			.expect_warning("⚠️ The substrate-contracts-node binary is not found.");

		let node_path = check_contracts_node_and_prompt(&mut cli, temp_dir.path(), false).await?;
		// node_path is path/substrate-contracts-node-v0.41.0 test only it starts with
		// path/substrate-contracts-node
		assert!(node_path
			.to_str()
			.unwrap()
			.starts_with(&temp_dir.path().join("substrate-contracts-node").to_str().unwrap()));
		Ok(())
	}
}
