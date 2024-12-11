// SPDX-License-Identifier: GPL-3.0

use cliclack::{confirm, log::warning, spinner};
use pop_common::{manifest::from_path, sourcing::set_executable_permission};
use pop_contracts::contracts_node_generator;
use std::{
	path::{Path, PathBuf},
	process::{Child, Command},
};
use tempfile::NamedTempFile;

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

			binary = contracts_node_generator(crate::cache()?, binary.latest()).await?;
			binary.source(false, &(), true).await?;
			set_executable_permission(binary.path())?;

			spinner.stop(format!(
				"✅ substrate-contracts-node successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			node_path = binary.path();
		}
	}

	Ok(node_path)
}

/// Handles the optional termination of a local running node.
pub fn terminate_node(process: Option<(Child, NamedTempFile)>) -> anyhow::Result<()> {
	// Prompt to close any launched node
	let Some((process, log)) = process else {
		return Ok(());
	};
	if confirm("Would you like to terminate the local node?")
		.initial_value(true)
		.interact()?
	{
		// Stop the process contracts-node
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;
	} else {
		log.keep()?;
		warning(format!("NOTE: The node is running in the background with process ID {}. Please terminate it manually when done.", process.id()))?;
	}

	Ok(())
}

/// Checks if a contract has been built by verifying the existence of the build directory and the
/// <name>.contract file.
///
/// # Arguments
/// * `path` - An optional path to the project directory. If no path is provided, the current
///   directory is used.
pub fn has_contract_been_built(path: Option<&Path>) -> bool {
	let project_path = path.unwrap_or_else(|| Path::new("./"));
	let Ok(manifest) = from_path(Some(project_path)) else {
		return false;
	};
	manifest
		.package
		.map(|p| project_path.join(format!("target/ink/{}.contract", p.name())).exists())
		.unwrap_or_default()
}

#[cfg(test)]
mod tests {
	use super::*;
	use duct::cmd;
	use std::fs::{self, File};

	#[test]
	fn has_contract_been_built_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(&path).run()?;
		let contract_path = path.join(name);
		assert!(!has_contract_been_built(Some(&contract_path)));

		cmd("cargo", ["build"]).dir(&contract_path).run()?;
		// Mock build directory
		fs::create_dir(&contract_path.join("target/ink"))?;
		assert!(!has_contract_been_built(Some(&path.join(name))));
		// Create a mocked .contract file inside the target directory
		File::create(contract_path.join(format!("target/ink/{}.contract", name)))?;
		assert!(has_contract_been_built(Some(&path.join(name))));
		Ok(())
	}
}
