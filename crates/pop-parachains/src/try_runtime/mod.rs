use crate::{errors::handle_command_error, Error};
use duct::cmd;
use frame_try_runtime::UpgradeCheckSelect;
use std::{fmt::Display, path::PathBuf};
use strum::Display;
use strum_macros::{AsRefStr, EnumMessage, EnumString, VariantArray};

/// Provides functionality for sourcing binaries of the `try-runtime-cli`.
pub mod binary;

/// Commands that can be executed by the `try-runtime-cli`.
pub enum TryRuntimeCliCommand {
	/// Command to test runtime migrations.
	OnRuntimeUpgrade,
}

impl Display for TryRuntimeCliCommand {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = match self {
			TryRuntimeCliCommand::OnRuntimeUpgrade => "on-runtime-upgrade",
		};
		write!(f, "{}", s)
	}
}

/// Subcommands for testing the runtime migrations.
#[derive(AsRefStr, Clone, Debug, EnumString, EnumMessage, VariantArray, Eq, PartialEq, Display)]
pub enum OnRuntimeUpgradeSubcommand {
	/// Run the migrations of a given runtime on top of a live state.
	#[strum(
		serialize = "live",
		message = "Live",
		detailed_message = "Run the migrations of a given runtime on top of a live state."
	)]
	Live,
	/// Run the migrations of a given runtime on top of a chain snapshot.
	#[strum(
		serialize = "snapshot",
		message = "Snapshot",
		detailed_message = "Run the migrations of a given runtime on top of a chain snapshot."
	)]
	Snapshot,
}

impl OnRuntimeUpgradeSubcommand {
	/// Get the command string for the `on-runtime-upgrade` subcommand.
	pub fn command(&self) -> String {
		match self {
			OnRuntimeUpgradeSubcommand::Live => "live",
			OnRuntimeUpgradeSubcommand::Snapshot => "snap",
		}
		.to_string()
	}
}

/// Get the details of upgrade checks options for testing the runtime migrations.
///
/// # Arguments
/// * `upgrade_check_select` - The selected upgrade check option.
pub fn get_upgrade_checks_details(upgrade_check_select: UpgradeCheckSelect) -> (String, String) {
	match upgrade_check_select {
		UpgradeCheckSelect::None => ("none".to_string(), "Run no checks".to_string()),
		UpgradeCheckSelect::All => (
			"all".to_string(),
			"Run the `try_state`, `pre_upgrade` and `post_upgrade` checks".to_string(),
		),
		UpgradeCheckSelect::TryState =>
			("try-state".to_string(), "Run the `try_state` checks".to_string()),
		UpgradeCheckSelect::PreAndPost => (
			"pre-and-post".to_string(),
			"Run the `pre_upgrade` and `post_upgrade` checks".to_string(),
		),
	}
}

/// Generates Try Runtime tests with `try-runtime-cli` binary.
///
/// # Arguments
/// * `binary_path` - Path to the binary.
/// * `command` - Command to run by the binary.
/// * `shared_params` - Shared parameters of the `try-runtime` command.
/// * `args` - Arguments passed to the subcommand.
/// * `excluded_args` - Arguments to exclude.
pub fn generate_try_runtime(
	binary_path: &PathBuf,
	command: TryRuntimeCliCommand,
	shared_params: Vec<String>,
	args: Vec<String>,
	excluded_args: &[&str],
) -> Result<(), Error> {
	let mut cmd_args = shared_params
		.into_iter()
		.filter(|arg| !excluded_args.iter().any(|a| arg.starts_with(a)))
		.collect::<Vec<String>>();
	cmd_args.extend(vec![command.to_string()]);
	cmd_args.extend(
		args.into_iter()
			.filter(|arg| !excluded_args.iter().any(|a| arg.starts_with(a)))
			.collect::<Vec<String>>(),
	);
	let output = cmd(binary_path, cmd_args)
		.env("RUST_LOG", "info")
		.stderr_capture()
		.unchecked()
		.run()?;
	// Check if the command failed.
	handle_command_error(&output, Error::TryRuntimeError)?;
	if output.status.success() {
		println!("{}", String::from_utf8_lossy(&output.stderr));
	}
	Ok(())
}
