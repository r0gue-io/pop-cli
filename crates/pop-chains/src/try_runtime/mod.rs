use crate::{Error, errors::handle_command_error};
use duct::cmd;
pub use frame_try_runtime::{TryStateSelect, UpgradeCheckSelect};
use std::{fmt::Display, path::PathBuf, str::from_utf8};

/// Provides functionality for sourcing binaries of the `try-runtime-cli`.
pub mod binary;
/// Provides functionality for parsing command-line arguments.
pub mod parse;
/// Shared parameters for the `try-runtime-cli` commands.
pub mod shared_parameters;
/// Types related to the source of runtime state.
pub mod state;

/// Commands that can be executed by the `try-runtime-cli`.
pub enum TryRuntimeCliCommand {
	/// Command to test runtime upgrades.
	OnRuntimeUpgrade,
	/// Command to test block execution.
	ExecuteBlock,
	/// Command to create a snapshot.
	CreateSnapshot,
	/// Command to mine a series of blocks after executing a runtime upgrade.
	FastForward,
}

impl Display for TryRuntimeCliCommand {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = match self {
			TryRuntimeCliCommand::OnRuntimeUpgrade => "on-runtime-upgrade",
			TryRuntimeCliCommand::ExecuteBlock => "execute-block",
			TryRuntimeCliCommand::CreateSnapshot => "create-snapshot",
			TryRuntimeCliCommand::FastForward => "fast-forward",
		};
		write!(f, "{}", s)
	}
}

/// Get the details of upgrade options for testing runtime upgrades.
///
/// # Arguments
/// * `upgrade_check_select` - The selected upgrade check option.
pub fn upgrade_checks_details(upgrade_check_select: &UpgradeCheckSelect) -> (String, String) {
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

/// Get the label of try state options.
///
/// # Arguments
/// * `try_state_select` - The selected try state option.
pub fn try_state_label(try_state_select: &TryStateSelect) -> String {
	match try_state_select {
		TryStateSelect::None => "None".to_string(),
		TryStateSelect::All => "All".to_string(),
		TryStateSelect::RoundRobin(..) => "Round Robin".to_string(),
		TryStateSelect::Only(..) => "Only Pallets".to_string(),
	}
}

/// Get the details of try state options for testing runtime upgrades.
///
/// # Arguments
/// * `try_state_select` - The selected try state option.
pub fn try_state_details(try_state_select: &TryStateSelect) -> (String, String) {
	(
		try_state_label(try_state_select),
		match try_state_select {
			TryStateSelect::None => "Run no tests".to_string(),
			TryStateSelect::All => "Run all the state tests".to_string(),
			TryStateSelect::RoundRobin(..) =>
				"Run a fixed number of state tests in a round robin manner.".to_string(),
			TryStateSelect::Only(..) =>
				"Run only pallets who's name matches the given list.".to_string(),
		},
	)
}

/// Parse the `try_state` to string.
///
/// # Arguments
/// * `try_state` - The selected try state option.
pub fn parse_try_state_string(try_state: &TryStateSelect) -> Result<String, Error> {
	Ok(match try_state {
		TryStateSelect::All => "all".to_string(),
		TryStateSelect::None => "none".to_string(),
		TryStateSelect::RoundRobin(rounds) => format!("rr-{}", rounds),
		TryStateSelect::Only(pallets) => {
			let mut result = vec![];
			for pallet in pallets.iter() {
				result.push(from_utf8(pallet).map_err(|_| {
					Error::ParamParsingError("Invalid pallet string in `try_state`".to_string())
				})?);
			}
			result.join(",")
		},
	})
}

/// Run `try-runtime-cli` binary.
///
/// # Arguments
/// * `binary_path` - Path to the binary.
/// * `command` - Command to run.
/// * `shared_params` - Shared parameters of the `try-runtime` command.
/// * `args` - Arguments passed to the subcommand.
/// * `excluded_args` - Arguments to exclude.
pub fn run_try_runtime(
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
		eprintln!("{}", String::from_utf8_lossy(&output.stderr));
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_try_state_string_works() {
		assert_eq!(parse_try_state_string(&TryStateSelect::All).unwrap(), "all");
		assert_eq!(parse_try_state_string(&TryStateSelect::None).unwrap(), "none");
		assert_eq!(parse_try_state_string(&TryStateSelect::RoundRobin(5)).unwrap(), "rr-5");
		assert_eq!(
			parse_try_state_string(&TryStateSelect::Only(vec![
				b"System".to_vec(),
				b"Proxy".to_vec()
			]))
			.unwrap(),
			"System,Proxy"
		);
	}

	#[test]
	fn try_state_label_works() {
		for (select, label) in [
			(TryStateSelect::All, "All"),
			(TryStateSelect::None, "None"),
			(TryStateSelect::RoundRobin(5), "Round Robin"),
			(TryStateSelect::Only(vec![]), "Only Pallets"),
		] {
			assert_eq!(try_state_label(&select), label);
		}
	}

	#[test]
	fn try_state_details_works() {
		for (select, description) in [
			(TryStateSelect::None, "Run no tests"),
			(TryStateSelect::All, "Run all the state tests"),
			(
				TryStateSelect::RoundRobin(0),
				"Run a fixed number of state tests in a round robin manner.",
			),
			(TryStateSelect::Only(vec![]), "Run only pallets who's name matches the given list."),
		] {
			assert_eq!(
				try_state_details(&select),
				(try_state_label(&select), description.to_string())
			);
		}
	}
}
