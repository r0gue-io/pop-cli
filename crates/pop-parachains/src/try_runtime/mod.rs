use crate::{errors::handle_command_error, Error};
use duct::cmd;
use frame_try_runtime::UpgradeCheckSelect;
use std::{fmt::Display, path::PathBuf};
use strum::{Display, EnumDiscriminants};
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

/// The source of runtime *state* to use.
#[derive(Clone, Debug, Display, clap::Subcommand, EnumDiscriminants)]
#[strum_discriminants(derive(AsRefStr, EnumString, EnumMessage, VariantArray))]
#[strum_discriminants(name(StateCommand))]
pub enum State {
	/// Use a live chain as the source of runtime state.
	#[strum_discriminants(strum(
		serialize = "live",
		message = "Live",
		detailed_message = "Run the migrations of a given runtime on top of a live state."
	))]
	Live(LiveState),

	/// Use a state snapshot as the source of runtime state.
	#[strum_discriminants(strum(
		serialize = "snapshot",
		message = "Snapshot",
		detailed_message = "Run the migrations of a given runtime on top of a chain snapshot."
	))]
	Snap {
		#[clap(short = 'p', long = "path", alias = "snapshot-path")]
		path: Option<PathBuf>,
	},
}

impl State {
	/// Get the command string for the `on-runtime-upgrade` subcommand.
	pub fn command(&self) -> String {
		match self {
			State::Live(..) => StateCommand::Live,
			State::Snap { .. } => StateCommand::Snap,
		}
		.to_string()
	}
}

impl Display for StateCommand {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = match self {
			StateCommand::Live => "live",
			StateCommand::Snap => "snap",
		};
		write!(f, "{}", s)
	}
}

/// A `Live` variant for [`State`]
#[derive(Debug, Clone, clap::Args)]
pub struct LiveState {
	/// The url to connect to.
	#[arg(
		short,
		long,
		value_parser = check_url,
	)]
	pub uri: Option<String>,

	/// The block hash at which to fetch the state.
	///
	/// If non provided, then the latest finalized head is used.
	#[arg(
		short,
		long,
		value_parser = check_block_hash,
	)]
	pub at: Option<String>,

	/// A pallet to scrape. Can be provided multiple times. If empty, entire chain state will
	/// be scraped.
	///
	/// This is equivalent to passing `xx_hash_64(pallet)` to `--hashed_prefixes`.
	#[arg(short, long, num_args = 1..)]
	pub pallet: Vec<String>,

	/// Storage entry key prefixes to scrape and inject into the test externalities. Pass as 0x
	/// prefixed hex strings. By default, all keys are scraped and included.
	#[arg(long = "prefix", value_parser = check_block_hash, num_args = 1..)]
	pub hashed_prefixes: Vec<String>,

	/// Fetch the child-keys as well.
	///
	/// Default is `false`, if specific `--pallets` are specified, `true` otherwise. In other
	/// words, if you scrape the whole state the child tree data is included out of the box.
	/// Otherwise, it must be enabled explicitly using this flag.
	#[arg(long)]
	pub child_tree: bool,
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

/// Checks if the given string is a valid URL.
///
/// # Arguments
///
/// * `s` - The string to check.
pub fn check_url(s: &str) -> Result<String, &'static str> {
	if s.starts_with("ws://") || s.starts_with("wss://") {
		// could use Url crate as well, but lets keep it simple for now.
		Ok(s.to_string())
	} else {
		Err("not a valid WS(S) url: must start with 'ws://' or 'wss://'")
	}
}

/// Checks if the given string is a valid block hash.
///
/// # Arguments
///
/// * `block_hash` - The string to check.
pub fn check_block_hash(block_hash: &str) -> anyhow::Result<String> {
	let (block_hash, offset) = if let Some(block_hash) = block_hash.strip_prefix("0x") {
		(block_hash, 2)
	} else {
		(block_hash, 0)
	};

	if let Some(pos) = block_hash.chars().position(|c| !c.is_ascii_hexdigit()) {
		Err(anyhow::anyhow!(
			"Expected block hash, found illegal hex character at position: {}",
			offset + pos,
		))
	} else {
		Ok(block_hash.into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn check_block_hash_works() {
		assert!(check_block_hash("0x1234567890abcdef").is_ok());
		assert!(check_block_hash("1234567890abcdef").is_ok());
		assert!(check_block_hash("0x1234567890abcdefg").is_err());
		assert!(check_block_hash("1234567890abcdefg").is_err());
	}
}
