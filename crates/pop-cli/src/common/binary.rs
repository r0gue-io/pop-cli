// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use cliclack::spinner;
use pop_common::sourcing::{set_executable_permission, Binary};
use std::path::{Path, PathBuf};

/// A trait for binary generator.
pub(crate) trait BinaryGenerator {
	/// Generates a binary.
	///
	/// # Arguments
	/// * `cache_path` - The cache directory path where the binary is stored.
	/// * `version` - The specific version used for the binary (`None` to fetch the latest version).
	async fn generate(
		cache_path: PathBuf,
		version: Option<&str>,
	) -> Result<Binary, pop_common::Error>;
}

/// Checks the status of the provided binary, sources it if necessary, and
/// prompts the user to update it if the existing binary is not the latest version.
///
/// # Arguments
/// * `cli` - Command-line interface for user interaction.
/// * `binary_name` - The name of the binary to check.
/// * `cache_path` - The cache directory path where the binary is stored.
/// * `skip_confirm` - If `true`, skips confirmation prompts and automatically sources the binary if
///   needed.
pub async fn check_and_prompt<Generator: BinaryGenerator>(
	cli: &mut impl Cli,
	binary_name: &'static str,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	let mut binary = Generator::generate(PathBuf::from(cache_path), None).await?;
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
			set_executable_permission(binary.path())?;

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

			binary = Generator::generate(crate::cache()?, binary.latest()).await?;
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

/// A macro to implement a binary generator.
#[macro_export]
macro_rules! impl_binary_generator {
	($generator_name:ident, $generate_fn:ident) => {
		pub(crate) struct $generator_name;

		impl BinaryGenerator for $generator_name {
			async fn generate(
				cache_path: PathBuf,
				version: Option<&str>,
			) -> Result<Binary, pop_common::Error> {
				$generate_fn(cache_path, version).await
			}
		}
	};
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{cli::MockCli, common::contracts::ContractsNodeGenerator};

	#[tokio::test]
	async fn check_binary_and_prompt_works() -> anyhow::Result<()> {
		#[cfg(feature = "wasm-contracts")]
		let binary_name = "substrate-contracts-node";
		#[cfg(feature = "polkavm-contracts")]
		let binary_name = "ink-node";
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning(format!("⚠️ The {binary_name} binary is not found."))
			.expect_confirm("📦 Would you like to source it automatically now?".to_string(), true)
			.expect_warning(format!("⚠️ The {binary_name} binary is not found."));

		let binary_path = check_and_prompt::<ContractsNodeGenerator>(
			&mut cli,
			binary_name,
			cache_path.path(),
			false,
		)
		.await?;

		// Binary path is at least equal to the cache path + `binary_name`.
		assert!(binary_path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join(binary_name).to_str().unwrap()));
		cli.verify()
	}

	#[tokio::test]
	async fn check_binary_and_prompt_handles_skip_confirm() -> anyhow::Result<()> {
		#[cfg(feature = "wasm-contracts")]
		let binary_name = "substrate-contracts-node";
		#[cfg(feature = "polkavm-contracts")]
		let binary_name = "ink-node";
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli =
			MockCli::new().expect_warning(format!("⚠️ The {binary_name} binary is not found."));

		let binary_path = check_and_prompt::<ContractsNodeGenerator>(
			&mut cli,
			binary_name,
			cache_path.path(),
			true,
		)
		.await?;
		// Binary path is at least equal to the cache path + `binary_name`.
		assert!(binary_path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join(binary_name).to_str().unwrap()));
		cli.verify()
	}
}
