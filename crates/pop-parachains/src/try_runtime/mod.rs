use crate::Error;
use duct::cmd;
use std::{fmt::Display, path::PathBuf};
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
#[derive(AsRefStr, Clone, Debug, EnumString, EnumMessage, VariantArray, Eq, PartialEq)]
pub enum Migration {
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
		message = "Live",
		detailed_message = "Run the migrations of a given runtime on top of a chain snapshot."
	)]
	Snapshot,
}

/// Generates binary benchmarks using `try-runtime`.
///
/// # Arguments
/// * `binary_path` - Path to the binary of FRAME Omni Bencher.
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
	let mut cmd_args = vec!["try-runtime".to_string(), command.to_string()];
	cmd_args.append(&mut args);

	if let Err(e) = cmd(binary_path, cmd_args).stderr_capture().run() {
		return Err(Error::BenchmarkingError(e.to_string()));
	}
	Ok(())
}
