// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::{Cli, Select},
	output::{CliResponse, OutputMode, PromptRequiredError},
};
use anyhow::{Context, Result};
use clap::Args;
#[cfg(not(test))]
use pop_common::{GitHub, polkadot_sdk::sort_by_latest_stable_version};
use serde::Serialize;
use std::{env, path::PathBuf};

#[cfg(not(test))]
const POLKADOT_SDK_GIT_SERVER: &str = "https://github.com/paritytech/polkadot-sdk";
const DEFAULT_GIT_SERVER: &str = "https://raw.githubusercontent.com";
const CARGO_TOML_FILE: &str = "Cargo.toml";

/// Arguments for upgrading the Polkadot SDK.
#[derive(Args, Serialize)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpgradeArgs {
	/// Path to the Cargo.toml file. If not provided, the current directory will be used.
	#[serde(skip_serializing)]
	#[arg(short, long)]
	pub(crate) path: Option<PathBuf>,
	/// Target Polkadot SDK version to switch to.
	/// If not specified, you will be prompted to select it.
	#[arg(short, long)]
	pub(crate) version: Option<String>,
}

// NOTE: this is a test-only function that mocks the network call to fetch available Polkadot SDK
// versions. It is used in tests to avoid network calls and exceeding the rate limit.
#[cfg(test)]
async fn fetch_polkadot_sdk_versions() -> Result<Vec<String>, anyhow::Error> {
	Ok(vec![
		"polkadot-stable2509-1".to_string(),
		"polkadot-stable2509".to_string(),
		"polkadot-stable2407-8".to_string(),
		"polkadot-stable2407-7".to_string(),
		"polkadot-stable2407-6".to_string(),
	])
}

#[cfg(not(test))]
async fn fetch_polkadot_sdk_versions() -> Result<Vec<String>, anyhow::Error> {
	let repo = GitHub::parse(POLKADOT_SDK_GIT_SERVER)?;
	let mut releases =
		repo.releases(false).await?.into_iter().map(|r| r.tag_name).collect::<Vec<_>>();
	sort_by_latest_stable_version(releases.as_mut_slice());
	Ok(releases)
}

/// Structured output for JSON mode.
#[derive(Serialize)]
struct UpgradeOutput {
	version: String,
	toml_path: String,
}

/// Resolves the path to the Cargo.toml file from the upgrade arguments.
fn resolve_toml_path(args: &UpgradeArgs) -> Result<PathBuf> {
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
	Ok(toml_file)
}

/// Entry point called from the command dispatcher.
pub(crate) async fn execute(args: &mut UpgradeArgs, output_mode: OutputMode) -> Result<()> {
	match output_mode {
		OutputMode::Human => Command::execute(args, &mut crate::cli::Cli).await,
		OutputMode::Json => {
			let version = args
				.version
				.as_ref()
				.ok_or_else(|| PromptRequiredError("--version is required with --json".into()))?
				.clone();
			let toml_file = resolve_toml_path(args)?;
			let crates_versions =
				psvm::get_version_mapping_with_fallback(DEFAULT_GIT_SERVER, &version)
					.await
					.map_err(|e| anyhow::anyhow!("Failed to get version mapping: {}", e))?;
			psvm::update_dependencies(&toml_file.canonicalize()?, &crates_versions, false, false)
				.map_err(|e| anyhow::anyhow!("Failed to update dependencies: {}", e))?;
			CliResponse::ok(UpgradeOutput { version, toml_path: toml_file.display().to_string() })
				.print_json();
			Ok(())
		},
	}
}

/// Upgrade command executor.
pub(crate) struct Command;

impl Command {
	/// Executes the polkadot-sdk version upgrade.
	pub(crate) async fn execute(args: &mut UpgradeArgs, cli: &mut impl Cli) -> Result<()> {
		cli.intro("Upgrade Polkadot SDK version")?;
		let toml_file = resolve_toml_path(args)?;

		cli.info(format!("Using {CARGO_TOML_FILE} file at {}", toml_file.display()))?;

		let version = if let Some(version) = &args.version {
			version.clone()
		} else {
			let spinner = cli.spinner();
			spinner.start("Fetching available Polkadot SDK versions...");
			let available_versions = fetch_polkadot_sdk_versions().await?;
			spinner.clear();
			let mut prompt = cli.select("Select the Polkadot SDK version (type to filter)");
			for version in &available_versions {
				prompt = prompt.item(version, version, "");
			}
			let version = prompt.filter_mode().interact()?.clone();
			args.version = Some(version.clone());
			version
		};

		let spinner = cli.spinner();
		spinner.start(format!("Updating dependencies to {}...", version));
		let crates_versions = psvm::get_version_mapping_with_fallback(DEFAULT_GIT_SERVER, &version)
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
		cli.success(format!("Polkadot SDK versions successfully upgraded to {}", version))?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{cli::MockCli, output::PromptRequiredError};
	use std::{
		fs,
		io::Write,
		path::{Path, PathBuf},
	};
	use tempfile::tempdir;

	fn write_minimal_cargo_toml(dir: &Path) -> PathBuf {
		fs::create_dir_all(dir).unwrap();
		let cargo_path = dir.join("Cargo.toml");
		let mut f = fs::File::create(&cargo_path).unwrap();
		writeln!(f, "[package]\nname = \"tmp_pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n")
			.unwrap();
		cargo_path
	}

	#[tokio::test]
	async fn execute_errors_when_cargo_toml_missing_for_directory_path() -> Result<()> {
		// Arrange: create an empty temporary directory without Cargo.toml
		let tmp = tempdir()?;
		fs::create_dir_all(&tmp)?;

		let mut cli = MockCli::new().expect_intro("Upgrade Polkadot SDK version");
		let mut args = UpgradeArgs {
			path: Some(tmp.path().to_path_buf()),
			version: Some("polkadot-stable2409-6".to_string()),
		};

		// Act
		let res = Command::execute(&mut args, &mut cli).await;

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
		cli.verify()?;
		Ok(())
	}

	#[tokio::test]
	async fn execute_errors_when_explicit_cargo_toml_path_does_not_exist() -> Result<()> {
		// Arrange: create a temp dir and point directly to a non-existent Cargo.toml file inside it
		let tmp = tempdir()?;
		fs::create_dir_all(&tmp)?;
		let cargo_path = tmp.path().join("Cargo.toml");

		let mut cli = MockCli::new().expect_intro("Upgrade Polkadot SDK version");
		let mut args = UpgradeArgs {
			path: Some(cargo_path),
			version: Some("polkadot-stable2509-1".to_string()),
		};

		// Act
		let res = Command::execute(&mut args, &mut cli).await;

		// Assert
		assert!(res.is_err(), "expected error when explicit Cargo.toml path does not exist");
		let msg = res.err().unwrap().to_string();
		assert!(
			msg.contains("Cargo.toml file not found at specified path"),
			"unexpected error: {}",
			msg
		);

		// Cleanup and verify CLI expectations
		fs::remove_dir_all(&tmp)?;
		cli.verify()?;
		Ok(())
	}

	#[tokio::test]
	async fn execute_prompts_for_version_and_sets_args_when_not_provided() -> Result<()> {
		// Arrange: temp workspace with minimal Cargo.toml
		let tmp = tempdir()?;
		let cargo_path = write_minimal_cargo_toml(tmp.path());
		let info_msg = format!("Using Cargo.toml file at {}", cargo_path.display());

		// Expected items
		let items = vec![
			("polkadot-stable2509-1".to_string(), "".to_string()),
			("polkadot-stable2509".to_string(), "".to_string()),
			("polkadot-stable2407-8".to_string(), "".to_string()),
			("polkadot-stable2407-7".to_string(), "".to_string()),
			("polkadot-stable2407-6".to_string(), "".to_string()),
		];

		let mut cli = MockCli::new()
			.expect_intro("Upgrade Polkadot SDK version")
			.expect_info(info_msg)
			.expect_select(
				"Select the Polkadot SDK version (type to filter)",
				None,
				true,
				Some(items.clone()),
				0, // choose the first item
				Some(true),
			);

		let mut args = UpgradeArgs { path: Some(tmp.path().to_path_buf()), version: None };

		// Act: this should perform selection, set args.version, then fail later on network mapping
		Command::execute(&mut args, &mut cli).await?;

		// Assert: we expect an error from version mapping stage, but args.version must be set
		assert_eq!(args.version, Some("polkadot-stable2509-1".to_string()));

		// Cleanup and verify CLI expectations
		fs::remove_dir_all(&tmp)?;
		cli.verify()?;
		Ok(())
	}

	#[tokio::test]
	async fn json_mode_requires_version_flag() -> Result<()> {
		let mut args = UpgradeArgs { path: None, version: None };
		let err = execute(&mut args, OutputMode::Json).await.unwrap_err();
		assert!(err.downcast_ref::<PromptRequiredError>().is_some());
		assert!(err.to_string().contains("--version is required with --json"));
		Ok(())
	}

	#[tokio::test]
	async fn json_mode_produces_valid_envelope() -> Result<()> {
		let tmp = tempdir()?;
		write_minimal_cargo_toml(tmp.path());
		let mut args = UpgradeArgs {
			path: Some(tmp.path().to_path_buf()),
			version: Some("polkadot-stable2509-1".to_string()),
		};
		// The execute call will succeed (psvm test mock returns empty mapping).
		execute(&mut args, OutputMode::Json).await?;

		// Verify the response shape by constructing the same envelope.
		let resp = CliResponse::ok(UpgradeOutput {
			version: "polkadot-stable2509-1".to_string(),
			toml_path: tmp.path().join("Cargo.toml").display().to_string(),
		});
		let json = serde_json::to_value(&resp).unwrap();
		assert_eq!(json["schema_version"], 1);
		assert_eq!(json["success"], true);
		assert_eq!(json["data"]["version"], "polkadot-stable2509-1");
		assert!(json.get("error").is_none());
		Ok(())
	}

	#[tokio::test]
	async fn execute_skips_prompt_when_version_is_provided() -> Result<()> {
		// Arrange: temp workspace with minimal Cargo.toml and a preselected version
		let tmp = tempdir()?;
		let cargo_path = write_minimal_cargo_toml(tmp.path());
		let info_msg = format!("Using Cargo.toml file at {}", cargo_path.display());

		let initial_version = "polkadot-stable2512".to_string();
		let mut cli = MockCli::new()
			.expect_intro("Upgrade Polkadot SDK version")
			.expect_info(info_msg);

		let mut args = UpgradeArgs {
			path: Some(tmp.path().to_path_buf()),
			version: Some(initial_version.clone()),
		};

		// Act: should not prompt for selection, go straight to mapping and fail there
		Command::execute(&mut args, &mut cli).await?;

		// Assert
		assert_eq!(args.version, Some(initial_version));

		// Cleanup and verify CLI expectations
		fs::remove_dir_all(&tmp)?;
		cli.verify()?;
		Ok(())
	}
}
