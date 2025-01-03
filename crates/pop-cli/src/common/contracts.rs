// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use cliclack::spinner;
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
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_contracts_node_and_prompt(
	cli: &mut impl Cli,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	let mut binary = contracts_node_generator(PathBuf::from(cache_path), None).await?;
	let mut node_path = binary.path();
	if !binary.exists() {
		cli.warning("⚠️ The substrate-contracts-node binary is not found.")?;
		let latest = if !skip_confirm {
			cli.confirm("📦 Would you like to source it automatically now?")
				.initial_value(true)
				.interact()?
		} else {
			true
		};
		if latest {
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
		let latest = if !skip_confirm {
			cli.confirm(
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
/// # Arguments
/// * `cli`: Command line interface.
/// * `process`: Tuple identifying the child process to terminate and its log file.
pub fn terminate_node(
	cli: &mut impl Cli,
	process: Option<(Child, NamedTempFile)>,
) -> anyhow::Result<()> {
	// Prompt to close any launched node
	let Some((process, log)) = process else {
		return Ok(());
	};
	if cli
		.confirm("Would you like to terminate the local node?")
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
		cli.warning(format!("NOTE: The node is running in the background with process ID {}. Please terminate it manually when done.", process.id()))?;
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
	use crate::cli::MockCli;
	use duct::cmd;
	use pop_common::find_free_port;
	use pop_contracts::{is_chain_alive, run_contracts_node};
	use std::fs::{self, File};
	use url::Url;

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

	#[tokio::test]
	async fn check_contracts_node_and_prompt_works() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning("⚠️ The substrate-contracts-node binary is not found.")
			.expect_confirm("📦 Would you like to source it automatically now?", true)
			.expect_warning("⚠️ The substrate-contracts-node binary is not found.");

		let node_path = check_contracts_node_and_prompt(&mut cli, cache_path.path(), false).await?;
		// Binary path is at least equal to the cache path + "substrate-contracts-node".
		assert!(node_path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join("substrate-contracts-node").to_str().unwrap()));
		cli.verify()
	}

	#[tokio::test]
	async fn check_contracts_node_and_prompt_handles_skip_confirm() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli =
			MockCli::new().expect_warning("⚠️ The substrate-contracts-node binary is not found.");

		let node_path = check_contracts_node_and_prompt(&mut cli, cache_path.path(), true).await?;
		// Binary path is at least equal to the cache path + "substrate-contracts-node".
		assert!(node_path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join("substrate-contracts-node").to_str().unwrap()));
		cli.verify()
	}

	#[tokio::test]
	async fn node_is_terminated() -> anyhow::Result<()> {
		let cache = tempfile::tempdir().expect("Could not create temp dir");
		let binary = contracts_node_generator(PathBuf::from(cache.path()), None).await?;
		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;
		let port = find_free_port(None);
		let process = run_contracts_node(binary.path(), None, port).await?;
		let log = NamedTempFile::new()?;
		// Terminate the process.
		let mut cli =
			MockCli::new().expect_confirm("Would you like to terminate the local node?", true);
		assert!(terminate_node(&mut cli, Some((process, log))).is_ok());
		assert_eq!(is_chain_alive(Url::parse(&format!("ws://localhost:{}", port))?).await?, false);
		cli.verify()
	}
}
