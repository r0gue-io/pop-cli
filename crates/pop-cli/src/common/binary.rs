// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use cliclack::spinner;
use pop_common::sourcing::{set_executable_permission, Binary};
use std::{
	future::Future,
	path::{Path, PathBuf},
};

pub struct BinaryBuilder<'a> {
	binary_name: &'a str,
}

impl<'a> BinaryBuilder<'a> {
	pub fn new(binary_name: &'a str) -> Self {
		Self { binary_name }
	}

	///  Checks the status of the provided binary, sources it if necessary, and
	/// prompts the user to update it if the existing binary is not the latest version.
	///
	/// # Arguments
	/// * `cli` - Command-line interface for user interaction.
	/// * `binary_name` - The name of the binary to check.
	/// * `binary_generator` - An asynchronous function that generates or retrieves the binary.
	/// * `cache_path` - The cache directory path where the binary is stored.
	/// * `skip_confirm` - If `true`, skips confirmation prompts and automatically sources the
	///   binary if needed.
	pub async fn check_and_prompt(
		&self,
		cli: &mut impl Cli,
		binary_generator: F,
		cache_path: &Path,
		skip_confirm: bool,
	) -> anyhow::Result<PathBuf>
	where
		F: Fn(PathBuf, Option<&str>) -> R + 'static,
		R: Future<Output = Result<Binary, pop_common::Error>> + Send,
	{
		let binary_name = self.binary_name;
		let mut binary = binary_generator(PathBuf::from(cache_path), None).await?;
		let mut binary_path = binary.path();
		if !binary.exists() {
			cli.warning(format!("⚠️ The {binary_name} binary is not found."))?;
			let latest = if !skip_confirm {
				cli.confirm("📦 Would you like to source it automatically now?")
					.initial_value(true)
					.interact()?
			} else {
				true
			};
			if latest {
				let spinner = spinner();
				spinner.start(format!("📦 Sourcing {binary_name}..."));

				binary.source(false, &(), true).await?;

				spinner.stop(format!(
					"✅ {binary_name} successfully sourced. Cached at: {}",
					binary.path().to_str().unwrap()
				));
				binary_path = binary.path();
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
				spinner.start(format!("📦 Sourcing {binary_name}..."));

				binary = binary_generator(crate::cache()?, binary.latest()).await?;
				binary.source(false, &(), true).await?;
				set_executable_permission(binary.path())?;

				spinner.stop(format!(
					"✅ {binary_name} successfully sourced. Cached at: {}",
					binary.path().to_str().unwrap()
				));
				binary_path = binary.path();
			}
		}

		Ok(binary_path)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use pop_contracts::contracts_node_generator;

	#[tokio::test]
	async fn check_binary_and_prompt_works() -> anyhow::Result<()> {
		let binary_name = "substrate-contracts-node";
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning(format!("⚠️ The {binary_name} binary is not found."))
			.expect_confirm(format!("📦 Would you like to source it automatically now?"), true)
			.expect_warning(format!("⚠️ The {binary_name} binary is not found."));

		let binary_path = BinaryBuilder::new(binary_name)
			.check_and_prompt(&mut cli, contracts_node_generator, cache_path.path(), true)
			.await?;

		// Binary path is at least equal to the cache path + `binary_name`.
		assert!(binary_path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join("substrate-contracts-node").to_str().unwrap()));
		cli.verify()
	}

	#[tokio::test]
	async fn check_binary_and_prompt_handles_skip_confirm() -> anyhow::Result<()> {
		let binary_name = "substrate-contracts-node";
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli =
			MockCli::new().expect_warning(format!("⚠️ The {binary_name} binary is not found."));

		let binary_path = BinaryBuilder::new(binary_name)
			.check_and_prompt(&mut cli, contracts_node_generator, cache_path.path(), true)
			.await?;
		// Binary path is at least equal to the cache path + `binary_name`.
		assert!(binary_path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join(binary_name).to_str().unwrap()));
		cli.verify()
	}
}
