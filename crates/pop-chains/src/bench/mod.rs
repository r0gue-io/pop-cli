// SPDX-License-Identifier: GPL-3.0

use crate::{Error, utils::helpers::HostFunctions};
use clap::Parser;
use duct::cmd;
use frame_benchmarking_cli::PalletCmd;
pub use frame_benchmarking_cli::{BlockCmd, MachineCmd, OverheadCmd, StorageCmd};
use serde::{Deserialize, Serialize};
use sp_runtime::traits::BlakeTwo256;
use std::{
	collections::BTreeMap,
	fmt::Display,
	io::Read,
	path::{Path, PathBuf},
};
use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};
use tempfile::NamedTempFile;

/// Provides functionality for sourcing binaries of the benchmarking CLI.
pub mod binary;

/// The default `development` preset used to communicate with the runtime via
/// [`GenesisBuilder`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html) interface.
///
/// (Recommended for testing with a single node, e.g., for benchmarking)
pub const GENESIS_BUILDER_DEV_PRESET: &str = "development";

/// Type alias for records where the key is the pallet name and the value is an array of its
/// extrinsics.
pub type PalletExtrinsicsRegistry = BTreeMap<String, Vec<String>>;

/// Commands that can be executed by the `frame-benchmarking-cli` CLI.
pub enum BenchmarkingCliCommand {
	/// Execute a pallet benchmark.
	Pallet,
	/// Execute an overhead benchmark.
	Overhead,
	/// Execute a storage benchmark.
	Storage,
	/// Execute a machine benchmark.
	Machine,
	/// Execute a block benchmark.
	Block,
}

impl Display for BenchmarkingCliCommand {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = match self {
			BenchmarkingCliCommand::Pallet => "pallet",
			BenchmarkingCliCommand::Overhead => "overhead",
			BenchmarkingCliCommand::Storage => "storage",
			BenchmarkingCliCommand::Machine => "machine",
			BenchmarkingCliCommand::Block => "block",
		};
		write!(f, "{}", s)
	}
}

/// How the genesis state for benchmarking should be built.
#[derive(
	clap::ValueEnum,
	Debug,
	Eq,
	PartialEq,
	Clone,
	Copy,
	EnumIter,
	EnumMessageDerive,
	Serialize,
	Deserialize,
)]
#[clap(rename_all = "kebab-case")]
pub enum GenesisBuilderPolicy {
	/// Do not provide any genesis state.
	None,
	/// Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API.
	Runtime,
}

impl Display for GenesisBuilderPolicy {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = match self {
			GenesisBuilderPolicy::None => "none",
			GenesisBuilderPolicy::Runtime => "runtime",
		};
		write!(f, "{}", s)
	}
}

impl TryFrom<String> for GenesisBuilderPolicy {
	type Error = String;

	fn try_from(s: String) -> Result<Self, Self::Error> {
		match s.as_str() {
			"none" => Ok(GenesisBuilderPolicy::None),
			"runtime" => Ok(GenesisBuilderPolicy::Runtime),
			_ => Err(format!("Invalid genesis builder policy: {}", s)),
		}
	}
}

/// Get the runtime folder path and throws error if it does not exist.
///
/// # Arguments
/// * `parent` - Parent path that contains the runtime folder.
pub fn get_runtime_path(parent: &Path) -> Result<PathBuf, Error> {
	["runtime", "runtimes"]
		.iter()
		.map(|f| parent.join(f))
		.find(|path| path.exists())
		.ok_or_else(|| Error::RuntimeNotFound(parent.to_str().unwrap().to_string()))
}

/// Runs pallet benchmarks using `frame-benchmarking-cli`.
///
/// # Arguments
/// * `args` - Arguments to pass to the benchmarking command.
pub fn generate_pallet_benchmarks(args: Vec<String>) -> Result<(), Error> {
	let cmd = PalletCmd::try_parse_from(std::iter::once("".to_string()).chain(args.into_iter()))
		.map_err(|e| Error::ParamParsingError(e.to_string()))?;

	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| Error::BenchmarkingError(e.to_string()))
}

/// Generates binary benchmarks using `frame-benchmarking-cli`.
///
/// # Arguments
/// * `binary_path` - Path to the binary of FRAME Omni Bencher.
/// * `command` - Command to run for benchmarking.
/// * `update_args` - Function to update the arguments before running the benchmark.
/// * `excluded_args` - Arguments to exclude from the benchmarking command.
pub fn generate_binary_benchmarks<F>(
	binary_path: &PathBuf,
	command: BenchmarkingCliCommand,
	update_args: F,
	excluded_args: &[&str],
) -> Result<String, Error>
where
	F: Fn(Vec<String>) -> Vec<String>,
{
	// Get all arguments of the command and skip the program name.
	let mut args = update_args(std::env::args().skip(3).collect::<Vec<String>>());
	args = args
		.into_iter()
		.filter(|arg| !excluded_args.iter().any(|a| arg.starts_with(a)))
		.collect::<Vec<String>>();
	let mut cmd_args = vec!["benchmark".to_string(), command.to_string()];
	cmd_args.append(&mut args);

	let stdout_file = NamedTempFile::new()?;
	let stdout_path = stdout_file.path().to_owned();

	if let Err(e) = cmd(binary_path, cmd_args).stdout_path(&stdout_path).stderr_capture().run() {
		return Err(Error::BenchmarkingError(e.to_string()));
	}

	let mut stdout_output = String::new();
	std::fs::File::open(&stdout_path)?.read_to_string(&mut stdout_output)?;
	Ok(stdout_output)
}

/// Loads a mapping of pallets and their associated extrinsics from the runtime binary.
///
/// # Arguments
/// * `runtime_path` - Path to the runtime binary.
/// * `binary_path` - Path to the binary of FRAME Omni Bencher.
pub async fn load_pallet_extrinsics(
	runtime_path: &Path,
	binary_path: &Path,
) -> Result<PalletExtrinsicsRegistry, Error> {
	let output = generate_omni_bencher_benchmarks(
		binary_path,
		BenchmarkingCliCommand::Pallet,
		vec![
			format!("--runtime={}", runtime_path.display()),
			"--genesis-builder=none".to_string(),
			"--list=all".to_string(),
		],
		false,
	)?;
	// Process the captured output and return the pallet extrinsics registry.
	Ok(process_pallet_extrinsics(output))
}

fn process_pallet_extrinsics(output: String) -> PalletExtrinsicsRegistry {
	// Process the captured output and return the pallet extrinsics registry.
	let mut registry = PalletExtrinsicsRegistry::new();
	let lines: Vec<String> = output.split("\n").map(String::from).skip(1).collect();
	for line in lines {
		if line.is_empty() {
			continue;
		}
		let record: Vec<String> = line.split(", ").map(String::from).collect();
		let pallet = record[0].trim().to_string();
		let extrinsic = record[1].trim().to_string();
		registry.entry(pallet).or_default().push(extrinsic);
	}

	// Sort the extrinsics by alphabetical order for each pallet.
	for extrinsics in registry.values_mut() {
		extrinsics.sort();
	}
	registry
}

/// Run command for benchmarking with a provided `frame-omni-bencher` binary.
///
/// # Arguments
/// * `binary_path` - Path to the binary to run.
/// * `command` - Command to run. `frame-omni-bencher` only supports `pallet` and `overhead`.
/// * `args` - Additional arguments to pass to the binary.
/// * `log_enabled` - Whether to enable logging.
pub fn generate_omni_bencher_benchmarks(
	binary_path: &Path,
	command: BenchmarkingCliCommand,
	args: Vec<String>,
	log_enabled: bool,
) -> Result<String, Error> {
	let stdout_file = NamedTempFile::new()?;
	let stdout_path = stdout_file.path().to_owned();

	let stderror_file = NamedTempFile::new()?;
	let stderror_path = stderror_file.path().to_owned();

	let mut cmd_args = vec!["v1".to_string(), "benchmark".to_string(), command.to_string()];
	cmd_args.extend(args);

	let cmd = cmd(binary_path, cmd_args)
		.env("RUST_LOG", if log_enabled { "info" } else { "none" })
		.stderr_path(&stderror_path)
		.stdout_path(&stdout_path);

	if let Err(e) = cmd.run() {
		let mut error_output = String::new();
		std::fs::File::open(&stderror_path)?.read_to_string(&mut error_output)?;
		return Err(Error::BenchmarkingError(
			if error_output.is_empty() { e.to_string() } else { error_output }
				.trim()
				.to_string(),
		));
	}

	let mut stdout_output = String::new();
	std::fs::File::open(&stdout_path)?.read_to_string(&mut stdout_output)?;
	Ok(stdout_output)
}

#[cfg(test)]
mod tests {
	use super::*;
	use binary::omni_bencher_generator;
	use std::fs;
	use tempfile::tempdir;

	#[test]
	fn get_runtime_path_works() -> Result<(), Error> {
		let temp_dir = tempdir()?;
		let path = temp_dir.path();
		let path_str = path.to_str().unwrap().to_string();

		assert_eq!(
			get_runtime_path(path).unwrap_err().to_string(),
			format!("Failed to find the runtime {}", path_str)
		);
		for name in ["runtime", "runtimes"] {
			fs::create_dir(path.join(name))?;
		}
		assert!(get_runtime_path(path).is_ok());
		Ok(())
	}

	#[tokio::test]
	async fn load_pallet_extrinsics_works() -> Result<(), Error> {
		let temp_dir = tempdir()?;
		let runtime_path = get_mock_runtime_path(true);
		let binary = omni_bencher_generator(temp_dir.path().to_path_buf(), None).await?;
		binary.source(false, &(), true).await?;

		let registry = load_pallet_extrinsics(&runtime_path, &binary.path()).await?;
		let pallets: Vec<String> = registry.keys().cloned().collect();
		assert_eq!(
			pallets,
			vec![
				"cumulus_pallet_parachain_system",
				"cumulus_pallet_xcmp_queue",
				"frame_system",
				"pallet_balances",
				"pallet_collator_selection",
				"pallet_message_queue",
				"pallet_session",
				"pallet_sudo",
				"pallet_timestamp"
			]
		);
		assert_eq!(
			registry.get("pallet_timestamp").cloned().unwrap_or_default(),
			["on_finalize", "set"]
		);
		assert_eq!(
			registry.get("pallet_sudo").cloned().unwrap_or_default(),
			["check_only_sudo_account", "remove_key", "set_key", "sudo", "sudo_as"]
		);
		Ok(())
	}

	#[tokio::test]
	async fn load_pallet_extrinsics_missing_runtime_benchmarks_fails() -> Result<(), Error> {
		let temp_dir = tempdir()?;
		let runtime_path = get_mock_runtime_path(false);
		let binary = omni_bencher_generator(temp_dir.path().to_path_buf(), None).await?;
		binary.source(false, &(), true).await?;

		assert_eq!(
			load_pallet_extrinsics(&runtime_path, &binary.path())
				.await
				.err()
				.unwrap()
				.to_string(),
			"Failed to run benchmarking: Error: Input(\"Did not find the benchmarking runtime api. This could mean that you either did not build the node correctly with the `--features runtime-benchmarks` flag, or the chain spec that you are using was not created by a node that was compiled with the flag\")"
		);
		Ok(())
	}

	fn get_mock_runtime_path(with_runtime_benchmarks: bool) -> PathBuf {
		let binary_path = format!(
			"../../tests/runtimes/{}.wasm",
			if with_runtime_benchmarks { "base_parachain_benchmark" } else { "base_parachain" }
		);
		std::env::current_dir().unwrap().join(binary_path).canonicalize().unwrap()
	}
}
