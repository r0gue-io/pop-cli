// SPDX-License-Identifier: GPL-3.0

use clap::Parser;
use frame_benchmarking_cli::{OpaqueBlock, OverheadCmd, PalletCmd};
use sc_chain_spec::GenesisConfigBuilderRuntimeCaller;
use sp_runtime::traits::BlakeTwo256;
use std::{
	collections::HashMap,
	fmt::Display,
	fs::{self, File},
	io::Read,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
};
use strum_macros::{EnumIter, EnumMessage as EnumMessageDerive};
use tempfile::NamedTempFile;

/// Provides functionality for sourcing binaries of the benchmarking CLI.
pub mod binary;

/// The default `development` preset used to communicate with the runtime via
/// [`GenesisBuilder`] interface.
///
/// (Recommended for testing with a single node, e.g., for benchmarking)
pub const GENESIS_BUILDER_DEV_PRESET: &str = "development";

type HostFunctions = (
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

/// Type alias for records where the key is the pallet name and the value is an array of its
/// extrinsics.
pub type PalletExtrinsicsRegistry = HashMap<String, Vec<String>>;

/// How the genesis state for benchmarking should be built.
#[derive(clap::ValueEnum, Debug, Eq, PartialEq, Clone, Copy, EnumIter, EnumMessageDerive)]
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

/// Get genesis builder preset names of the runtime.
///
/// # Arguments
/// * `binary_path` - Path to the runtime binary.
pub fn get_preset_names(binary_path: &PathBuf) -> anyhow::Result<Vec<String>> {
	let binary = fs::read(binary_path)?;
	let genesis_config_builder = GenesisConfigBuilderRuntimeCaller::<HostFunctions>::new(&binary);
	genesis_config_builder.preset_names().map_err(|e| anyhow::anyhow!(e))
}

/// Get the runtime folder path and throws error if it does not exist.
///
/// # Arguments
/// * `parent` - Parent path that contains the runtime folder.
pub fn get_runtime_path(parent: &Path) -> anyhow::Result<PathBuf> {
	["runtime", "runtimes"]
		.iter()
		.map(|f| parent.join(f))
		.find(|path| path.exists())
		.ok_or_else(|| anyhow::anyhow!("No runtime found"))
}

/// Runs pallet benchmarks using `frame-benchmarking-cli`.
///
/// # Arguments
/// * `args` - Arguments to pass to the benchmarking command.
pub fn generate_benchmarks(args: Vec<String>) -> anyhow::Result<()> {
	let cmd = PalletCmd::try_parse_from([vec!["".to_string()], args].concat())
		.map_err(|e| anyhow::anyhow!("Invalid command arguments: {}", e))?;
	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| anyhow::anyhow!("Failed to run benchmarking: {}", e))
}

/// Run command for overhead benchmarking.
///
/// # Arguments
/// * `cmd` - Command to benchmark the execution overhead per-block and per-extrinsic.
pub async fn generate_overhead_benchmarks(cmd: OverheadCmd) -> anyhow::Result<()> {
	tokio::task::spawn_blocking(move || {
		std::env::set_var("RUST_LOG", "info");
		let _ = env_logger::try_init();
		cmd.run_with_default_builder_and_spec::<OpaqueBlock, HostFunctions>(None)
			.map_err(|e| anyhow::anyhow!(format!("Failed to run benchmarking: {}", e)))
	})
	.await?
}

/// Loads a mapping of pallets and their associated extrinsics from the runtime binary.
///
/// # Arguments
/// * `runtime_path` - Path to the runtime binary.
/// * `binary_path` - Path to the binary of FRAME Omni Bencher.
pub async fn load_pallet_extrinsics(
	runtime_path: &Path,
	binary_path: &Path,
) -> anyhow::Result<PalletExtrinsicsRegistry> {
	let temp_file = NamedTempFile::new()?;

	let mut process = run_benchmarking_with_binary(
		binary_path,
		Some(temp_file.as_file()),
		vec![
			&format!("--runtime={}", runtime_path.display()),
			"--genesis-builder=none",
			"--list=all",
		],
	)
	.await?;

	// Wait for the process to finish and retain the output file.
	let (_, path) = temp_file.keep()?;
	process.wait()?;

	// Process the captured output and return the pallet extrinsics registry.
	process_pallet_extrinsics(path)
}

fn process_pallet_extrinsics(output_file: PathBuf) -> anyhow::Result<PalletExtrinsicsRegistry> {
	let mut output_file = File::open(output_file)?;
	let mut output = String::new();
	output_file.read_to_string(&mut output)?;

	// Returns an error if the runtime is not built with `--features runtime-benchmarks`.
	if output.contains("--features runtime-benchmarks") {
		return Err(anyhow::anyhow!("Runtime is not built with `--features runtime-benchmarks`. Please rebuild it with the feature enabled."));
	}

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
	Ok(registry)
}

/// Run command for benchmarking with a provided binary.
///
/// # Arguments
/// * `binary_path` - Path to the binary to run.
/// * `output` - Output file to write the benchmark results to.
/// * `args` - Additional arguments to pass to the binary.
pub async fn run_benchmarking_with_binary(
	binary_path: &Path,
	output: Option<&File>,
	args: Vec<&str>,
) -> anyhow::Result<Child> {
	let mut command = Command::new(binary_path);
	let env = std::env::var("RUST_LOG").unwrap_or_default();
	command.env("RUST_LOG", "none");
	command.args(["v1", "benchmark", "pallet"]);
	for arg in args {
		command.arg(arg);
	}
	if let Some(output) = output {
		command.stdout(Stdio::from(output.try_clone()?));
		command.stderr(Stdio::from(output.try_clone()?));
	}
	let child = command.spawn()?;
	command.env("RUST_LOG", env);
	Ok(child)
}

#[cfg(test)]
mod tests {
	use super::*;
	use binary::omni_bencher_generator;
	use tempfile::tempdir;

	#[test]
	fn generate_benchmarks_works() -> anyhow::Result<()> {
		generate_benchmarks(vec![
			"--pallet=pallet_timestamp".to_string(),
			"--extrinsic=*".to_string(),
			"--runtime".to_string(),
			get_mock_runtime_path(true).to_str().unwrap().to_string(),
		])
	}

	#[tokio::test]
	async fn generate_overhead_benchmarks_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let output_path = temp_dir.path().to_str().unwrap();
		generate_overhead_benchmarks(OverheadCmd::try_parse_from(vec![
			"--warmup=1",
			"--repeat=1",
			"--runtime",
			get_mock_runtime_path(true).to_str().unwrap(),
			"--weight-path",
			output_path,
		])?)
		.await
	}

	#[test]
	fn get_preset_names_works() -> anyhow::Result<()> {
		assert_eq!(
			get_preset_names(&get_mock_runtime_path(true))?,
			vec!["development", "local_testnet"]
		);
		Ok(())
	}

	#[test]
	fn get_runtime_path_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		for name in ["runtime", "runtimes"] {
			let path = temp_dir.path();
			fs::create_dir(&path.join(name))?;
			get_runtime_path(&path)?;
		}
		Ok(())
	}

	#[tokio::test]
	async fn load_pallet_extrinsics_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtime_path = get_mock_runtime_path(true);
		let binary = omni_bencher_generator(temp_dir.path(), None).await?;
		binary.source(false, &(), true).await?;

		let registry = load_pallet_extrinsics(&runtime_path, &binary.path()).await?;
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
	async fn load_pallet_extrinsics_missing_runtime_benchmarks_fails() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let runtime_path = get_mock_runtime_path(false);
		let binary = omni_bencher_generator(temp_dir.path(), None).await?;
		binary.source(false, &(), true).await?;

		assert_eq!(
		    load_pallet_extrinsics(&runtime_path, &binary.path()).await.err().unwrap().to_string(),
			"Runtime is not built with `--features runtime-benchmarks`. Please rebuild it with the feature enabled."
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
