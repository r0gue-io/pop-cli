// SPDX-License-Identifier: GPL-3.0

use clap::Parser;
use frame_benchmarking_cli::PalletCmd;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
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

/// Type alias for records where the key is the pallet name and the value is a array of its
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

impl From<String> for GenesisBuilderPolicy {
	fn from(s: String) -> Self {
		match s {
			s if s == *"none" => GenesisBuilderPolicy::None,
			s if s == *"runtime" => GenesisBuilderPolicy::Runtime,
			_ => unreachable!(),
		}
	}
}

/// Get genesis builder preset names of the runtime.
///
/// # Arguments
/// * `binary_path` - Path to the runtime WASM binary.
pub fn get_preset_names(binary_path: &PathBuf) -> anyhow::Result<Vec<String>> {
	let binary = fs::read(binary_path).expect("No runtime binary found");
	let genesis_config_builder = GenesisConfigBuilderRuntimeCaller::<HostFunctions>::new(&binary);
	genesis_config_builder.preset_names().map_err(|e| anyhow::anyhow!(e))
}

/// Get the runtime folder path and throws error if not exist.
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

/// Runs FRAME pallet benchmarks using `frame-benchmarking-cli`.
///
/// # Arguments
/// * `args` - Arguments to pass to the benchmarking command.
pub fn generate_benchmarks(args: Vec<String>) -> anyhow::Result<()> {
	let cmd = PalletCmd::try_parse_from([vec!["".to_string()], args].concat())
		.map_err(|e| anyhow::anyhow!("Invalid command arguments: {}", e))?;
	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| anyhow::anyhow!("Failed to run benchmarking: {}", e))
}

/// Loads a mapping of pallets and their associated extrinsics from the runtime WASM binary.
///
/// # Arguments
/// * `runtime_path` - Path to the runtime WASM binary.
/// * `binary_path` - Path to the binary of FRAME Omni Bencher.
pub async fn load_pallet_extrinsics(
	runtime_path: &Path,
	binary_path: &Path,
) -> anyhow::Result<PalletExtrinsicsRegistry> {
	let temp_file = NamedTempFile::new()?;

	let mut process = run_benchmarking_with_binary(
		binary_path,
		Some(temp_file.as_file()),
		vec![&format!("--runtime={}", runtime_path.display()), "--list=all"],
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
	command.env("RUST_LOG", "none");
	command.args(["v1", "benchmark", "pallet"]);
	for arg in args {
		command.arg(arg);
	}
	if let Some(output) = output {
		command.stdout(Stdio::from(output.try_clone()?));
		command.stderr(Stdio::from(output.try_clone()?));
	}
	Ok(command.spawn()?)
}

/// Performs a fuzzy search for pallets that match the provided input.
///
/// # Arguments
/// * `registry` - A mapping of pallets and their extrinsics.
/// * `excluded_pallets` - Pallets that are excluded from the search results.
/// * `input` - The search input used to match pallets.
/// * `limit` - Maximum number of pallets returned from search.
pub fn search_for_pallets(
	registry: &PalletExtrinsicsRegistry,
	excluded_pallets: &[String],
	input: &str,
	limit: usize,
) -> Vec<String> {
	let matcher = SkimMatcherV2::default();
	let pallets = registry.keys();

	if input.is_empty() {
		return pallets.map(String::from).take(limit).collect();
	}
	let pallets: Vec<&str> = pallets
		.filter(|s| !excluded_pallets.contains(&s.to_string()))
		.map(String::as_str)
		.collect();
	let mut output: Vec<(String, i64)> = pallets
		.into_iter()
		.map(|v| (v.to_string(), matcher.fuzzy_match(v, input).unwrap_or_default()))
		.collect();
	// Sort pallets by score.
	output.sort_by(|a, b| b.1.cmp(&a.1));
	output.into_iter().map(|(name, _)| name).take(limit).collect::<Vec<String>>()
}

/// Performs a fuzzy search for extrinsics that match the provided input.
///
/// # Arguments
/// * `registry` - A mapping of pallets and their extrinsics.
/// * `pallet` - Pallet to find the extrinsics.
/// * `input` - The search input used to match extrinsics.
/// * `limit` - The maximum number of results to return.
pub fn search_for_extrinsics(
	registry: &PalletExtrinsicsRegistry,
	pallet: &String,
	input: &str,
	limit: usize,
) -> Vec<String> {
	let matcher = SkimMatcherV2::default();
	let extrinsics = registry.get(pallet).cloned().unwrap_or_default();

	if input.is_empty() {
		return extrinsics.into_iter().take(limit).collect();
	}
	let mut output: Vec<(String, i64)> = extrinsics
		.into_iter()
		.map(|v| (v.clone(), matcher.fuzzy_match(&v, input).unwrap_or_default()))
		.collect();
	// Sort extrinsics by score.
	output.sort_by(|a, b| b.1.cmp(&a.1));
	output.into_iter().map(|(name, _)| name).take(limit).collect::<Vec<String>>()
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

	#[test]
	fn search_pallets_works() {
		let registry = get_mock_registry();
		[
			("balances", "pallet_balances"),
			("timestamp", "pallet_timestamp"),
			("system", "frame_system"),
		]
		.iter()
		.for_each(|(input, pallet)| {
			let pallets = search_for_pallets(&registry, &[], input, 5);
			assert_eq!(pallets.first(), Some(&pallet.to_string()));
			assert_eq!(pallets.len(), 3);
		});

		assert_ne!(
			search_for_pallets(&registry, &["pallet_timestamp".to_string()], "timestamp", 5)
				.first(),
			Some(&"pallet_timestamp".to_string())
		);
	}

	#[test]
	fn search_extrinsics_works() {
		let registry = get_mock_registry();
		// Extrinsics are sorted alphabetically if there are no matches.
		assert_eq!(
			search_for_extrinsics(&registry, &"pallet_timestamp".to_string(), "", 5),
			vec!["on_finalize".to_string(), "set".to_string()]
		);
		// Sort by score if there are matches.
		assert_eq!(
			search_for_extrinsics(&registry, &"pallet_timestamp".to_string(), "set", 5),
			vec!["set".to_string(), "on_finalize".to_string()]
		);
	}

	fn get_mock_runtime_path(with_runtime_benchmarks: bool) -> PathBuf {
		let binary_path = format!(
			"../../tests/runtimes/{}.wasm",
			if with_runtime_benchmarks { "base_parachain_benchmark" } else { "base_parachain" }
		);
		std::env::current_dir().unwrap().join(binary_path).canonicalize().unwrap()
	}

	fn get_mock_registry() -> PalletExtrinsicsRegistry {
		PalletExtrinsicsRegistry::from([
			(
				"pallet_balances".to_string(),
				vec![
					"transfer".to_string(),
					"force_transfer".to_string(),
					"set_balance".to_string(),
				],
			),
			("pallet_timestamp".to_string(), vec!["on_finalize".to_string(), "set".to_string()]),
			("frame_system".to_string(), vec!["set_code".to_string(), "remark".to_string()]),
		])
	}
}
