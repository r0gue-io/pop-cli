// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::Cli;
use anyhow::{Context, Result};
use clap::Args;
use cliclack::spinner;
use psvm::get_version_mapping_with_fallback;
use serde::Serialize;
use std::{env, path::PathBuf};

const DEFAULT_GIT_SERVER: &str = "https://raw.githubusercontent.com";
const CARGO_TOML_FILE: &str = "Cargo.toml";

/// Arguments for upgrading the Polkadot SDK.
#[derive(Args, Serialize)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpgradeArgs {
	/// Path to the Cargo.toml file. If not provided, the current directory will be used.
	#[arg(short, long)]
	pub(crate) path: Option<PathBuf>,
	/// Target Polkadot SDK version to switch to (default: 0.3.0).
	#[arg(short, long)]
	pub(crate) version: String,
}

/// Upgrade command executor.
pub(crate) struct Command;

impl Command {
	/// Executes the polkadot-sdk version upgrade.
	pub(crate) async fn execute(args: &UpgradeArgs, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Upgrade Polkadot SDK version")?;
		let toml_file = if let Some(path) = &args.path {
			if matches!(path.file_name().map(|n| n.to_str().unwrap()), Some(CARGO_TOML_FILE)) {
				path.clone()
			} else {
				path.join(CARGO_TOML_FILE)
			}
		} else {
			let current_dir = env::current_dir().context("Failed to get current directory")?;
			current_dir.join(CARGO_TOML_FILE)
		};
		if !toml_file.exists() {
			anyhow::bail!("{CARGO_TOML_FILE} file not found at specified path");
		}

		cli.info(format!("Using {CARGO_TOML_FILE} file at {}", toml_file.display()))?;

		let spinner = spinner();
		spinner.start(format!("Updating dependencies to {}...", args.version));
		let crates_versions = get_version_mapping_with_fallback(DEFAULT_GIT_SERVER, &args.version)
			.await
			.map_err(|e| anyhow::anyhow!("Failed to get version mapping: {}", e))?;

		psvm::update_dependencies(&toml_file.canonicalize()?, &crates_versions, false, false)
			.map_err(|e| anyhow::anyhow!("Failed to update dependencies: {}", e))?;

		spinner.stop("Upgrade complete");

		cli.warning(
			"After upgrade to a new polkadot-sdk version, you may encounter compilation errors. \
		 Run `pop build` to check if the project compiles successfully. \
		 If it does not, fix the compilation errors and try again.",
		)?;
		cli.success(format!("Polkadot SDK versions successfully upgraded to {}", args.version))?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use std::{
		fs,
		path::PathBuf,
		time::{SystemTime, UNIX_EPOCH},
	};

	fn unique_temp_dir() -> PathBuf {
		let mut dir = std::env::temp_dir();
		let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
		dir.push(format!("pop_cli_upgrade_test_{}", nanos));
		dir
	}

	#[tokio::test]
	async fn execute_errors_when_cargo_toml_missing_for_directory_path() {
		// Arrange: create an empty temporary directory without Cargo.toml
		let tmp = unique_temp_dir();
		fs::create_dir_all(&tmp).unwrap();

		let mut cli = MockCli::new().expect_intro("Upgrade Polkadot SDK version");
		let args = UpgradeArgs { path: Some(tmp.clone()), version: "0.3.0".to_string() };

		// Act
		let res = Command::execute(&args, &mut cli).await;

		// Assert: we should fail before any network calls trying to read version mapping
		assert!(res.is_err(), "expected error when Cargo.toml is missing");
		let msg = res.err().unwrap().to_string();
		assert!(
			msg.contains("Cargo.toml file not found at specified path"),
			"unexpected error: {}",
			msg
		);

		// Cleanup and verify CLI expectations
		let _ = fs::remove_dir_all(&tmp);
		cli.verify().unwrap();
	}

	#[tokio::test]
	async fn execute_errors_when_explicit_cargo_toml_path_does_not_exist() {
		// Arrange: create a temp dir and point directly to a non-existent Cargo.toml file inside it
		let tmp = unique_temp_dir();
		fs::create_dir_all(&tmp).unwrap();
		let cargo_path = tmp.join("Cargo.toml");

		let mut cli = MockCli::new().expect_intro("Upgrade Polkadot SDK version");
		let args = UpgradeArgs { path: Some(cargo_path), version: "0.4.0".to_string() };

		// Act
		let res = Command::execute(&args, &mut cli).await;

		// Assert
		assert!(res.is_err(), "expected error when explicit Cargo.toml path does not exist");
		let msg = res.err().unwrap().to_string();
		assert!(
			msg.contains("Cargo.toml file not found at specified path"),
			"unexpected error: {}",
			msg
		);

		// Cleanup and verify CLI expectations
		let _ = fs::remove_dir_all(&tmp);
		cli.verify().unwrap();
	}
}
