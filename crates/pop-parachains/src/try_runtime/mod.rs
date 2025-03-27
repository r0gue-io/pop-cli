use crate::{errors::handle_command_error, Error};
use duct::cmd;
use frame_try_runtime::UpgradeCheckSelect;
use std::{fmt::Display, path::PathBuf};
use strum::Display;
use strum_macros::{AsRefStr, EnumMessage, EnumString, VariantArray};

/// Provides functionality for sourcing binaries of the try-runtime CLI.
pub mod binary;

/// Commands that can be executed by the `frame-benchmarking-cli` CLI.
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

/// Chain state options for testing the runtime migrations.
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

/// Generates binary benchmarks using `try-runtime`.
///
/// # Arguments
/// * `binary_path` - Path to the binary of Try Runtime CLI.
/// * `command` - Command to run for benchmarking.
/// * `update_args` - Function to update the arguments before running the benchmark.
/// * `excluded_args` - Arguments to exclude from the benchmarking command.
pub fn generate_try_runtime<F>(
	binary_path: &PathBuf,
	command: TryRuntimeCliCommand,
	update_args: F,
	excluded_args: &[&str],
) -> Result<(), Error>
where
	F: Fn(Vec<String>) -> Vec<String>,
{
	// Get all arguments of the command and skip the program name.
	let mut args = update_args(std::env::args().skip(3).collect::<Vec<String>>());
	args = args
		.into_iter()
		.filter(|arg| !excluded_args.iter().any(|a| arg.starts_with(a)))
		.collect::<Vec<String>>();
	let mut cmd_args = vec![command.to_string()];
	cmd_args.append(&mut args);

	let output = cmd(binary_path, cmd_args).stderr_capture().unchecked().run()?;
	// Check if the command failed.
	handle_command_error(&output, Error::TryRuntimeError)?;
	Ok(())
}
