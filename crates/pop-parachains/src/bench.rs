use anyhow::Result;
use clap::Parser;
use csv::Reader;
use frame_benchmarking_cli::PalletCmd;
use rust_fuzzy_search::fuzzy_search_best_n;
use sc_chain_spec::GenesisConfigBuilderRuntimeCaller;
use sp_runtime::traits::BlakeTwo256;
use std::{
	collections::HashMap,
	fs,
	fs::File,
	io::BufReader,
	path::{Path, PathBuf},
};
use stdio_override::StdoutOverride;
use tempfile::tempdir;

/// Constant variables used for benchmarking.
pub mod constants {
	/// Do not provide any genesis state.
	pub const GENESIS_BUILDER_NO_POLICY: &str = "none";
	/// Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API.
	pub const GENESIS_BUILDER_RUNTIME_POLICY: &str = "runtime";
}

type HostFunctions = (
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

/// Type alias for records where the key is the pallet name and the value is a array of its
/// extrinsics.
pub type PalletExtrinsicsCollection = HashMap<String, Vec<String>>;

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
		.ok_or_else(|| anyhow::anyhow!("No runtime found."))
}

/// List a mapping of pallets and their extrinsics.
///
/// # Arguments
/// * `runtime_path` - Path to the runtime WASM binary.
pub fn list_pallets_and_extrinsics(
	runtime_path: &Path,
) -> anyhow::Result<PalletExtrinsicsCollection> {
	let temp_dir = tempdir()?;
	let temp_file_path = temp_dir.path().join("pallets.csv");
	let guard = StdoutOverride::from_file(&temp_file_path)?;
	let cmd = PalletCmd::try_parse_from([
		"",
		"--runtime",
		runtime_path.to_str().unwrap(),
		"--genesis-builder",
		"none", // For parsing purpose.
		"--list=all",
	])?;
	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| anyhow::anyhow!(format!("Failed to list pallets: {}", e.to_string())))?;
	drop(guard);
	parse_csv_to_map(&temp_file_path)
}

/// Print the pallet benchmarking command with arguments.
///
/// # Arguments
/// * `cmd` - Command to benchmarking extrinsic weights of FRAME pallets.
pub fn print_pallet_command(cmd: &PalletCmd) -> String {
	let mut full_message = "pop bench pallet".to_string();

	if let Some(ref pallet) = cmd.pallet {
		full_message.push_str(&format!(" --pallet={}", pallet));
	}
	if let Some(ref extrinsic) = cmd.extrinsic {
		full_message.push_str(&format!(" --extrinsic={}", extrinsic));
	}
	if !cmd.exclude_pallets.is_empty() {
		full_message.push_str(&format!(" --exclude-pallets={}", cmd.exclude_pallets.join(",")));
	}
	full_message.push_str(&format!(" --steps={}", cmd.steps));
	if !cmd.lowest_range_values.is_empty() {
		let low = cmd
			.lowest_range_values
			.iter()
			.map(ToString::to_string)
			.collect::<Vec<_>>()
			.join(", ");
		full_message.push_str(&format!(" --low={}", low));
	}
	if !cmd.highest_range_values.is_empty() {
		let high = cmd
			.highest_range_values
			.iter()
			.map(ToString::to_string)
			.collect::<Vec<_>>()
			.join(", ");
		full_message.push_str(&format!(" --high={}", high));
	}
	full_message.push_str(&format!(" --repeat={}", cmd.repeat));
	full_message.push_str(&format!(" --external-repeat={}", cmd.external_repeat));
	if cmd.json_output {
		full_message.push_str(" --json");
	}
	if let Some(ref json_file) = cmd.json_file {
		full_message.push_str(&format!(" --json-file={}", json_file.display()));
	}
	if cmd.no_median_slopes {
		full_message.push_str(" --no-median-slopes");
	}
	if cmd.no_min_squares {
		full_message.push_str(" --no-min-squares");
	}
	if let Some(ref output) = cmd.output {
		full_message.push_str(&format!(" --output={}", output.display()));
	}
	if let Some(ref header) = cmd.header {
		full_message.push_str(&format!(" --header={}", header.display()));
	}
	if let Some(ref template) = cmd.template {
		full_message.push_str(&format!(" --template={}", template.display()));
	}
	if let Some(ref output_analysis) = cmd.output_analysis {
		full_message.push_str(&format!(" --output-analysis={}", output_analysis));
	}
	if let Some(ref output_pov_analysis) = cmd.output_pov_analysis {
		full_message.push_str(&format!(" --output-pov-analysis={}", output_pov_analysis));
	}
	if let Some(ref heap_pages) = cmd.heap_pages {
		full_message.push_str(&format!(" --heap-pages={}", heap_pages));
	}
	if cmd.no_verify {
		full_message.push_str(" --no-verify");
	}
	if cmd.extra {
		full_message.push_str(" --extra");
	}
	if let Some(ref runtime) = cmd.runtime {
		full_message.push_str(&format!(" --runtime={}", runtime.display()));
	}
	if cmd.allow_missing_host_functions {
		full_message.push_str(" --allow-missing-host-functions");
	}
	if let Some(ref genesis_builder) = cmd.genesis_builder {
		let genesis_builder_string = serde_json::to_string(genesis_builder).unwrap().to_lowercase();
		full_message.push_str(&format!(" --genesis-builder={}", genesis_builder_string));
		if genesis_builder_string == constants::GENESIS_BUILDER_RUNTIME_POLICY {
			full_message
				.push_str(&format!(" --genesis-builder-preset {}", cmd.genesis_builder_preset));
		}
	}
	if let Some(ref execution) = cmd.execution {
		full_message.push_str(&format!(" --execution={}", execution));
	}
	full_message.push_str(&format!(" --db-cache={}", cmd.database_cache_size));
	if cmd.no_storage_info {
		full_message.push_str(" --no-storage-info");
	}
	full_message.push_str(&format!(" --map-size={}", cmd.worst_case_map_values));
	full_message.push_str(&format!(" --additional-trie-layers={}", cmd.additional_trie_layers));
	if let Some(ref json_input) = cmd.json_input {
		full_message.push_str(&format!(" --json-input={}", json_input.display()));
	}
	if cmd.unsafe_overwrite_results {
		full_message
			.push_str(&format!(" --unsafe-overwrite-results={}", cmd.unsafe_overwrite_results));
	}
	full_message
}

/// Parse the pallet command from string value of genesis policy builder.
///
/// # Arguments
/// * `policy` - Genesis builder policy ( none | spec | runtime ).
pub fn parse_genesis_builder_policy(policy: &str) -> anyhow::Result<PalletCmd> {
	PalletCmd::try_parse_from([
		"",
		"--list",
		"--runtime",
		"dummy-runtime", // For parsing purpose.
		"--genesis-builder",
		policy,
	])
	.map_err(|e| {
		anyhow::anyhow!(format!(r#"Invalid genesis builder option {policy}: {}"#, e.to_string()))
	})
}

fn parse_csv_to_map(file_path: &PathBuf) -> anyhow::Result<PalletExtrinsicsCollection> {
	let file = File::open(file_path)?;
	let mut rdr = Reader::from_reader(BufReader::new(file));
	let mut map: PalletExtrinsicsCollection = HashMap::new();
	for result in rdr.records() {
		let record = result?;
		if record.len() == 2 {
			let pallet = record[0].trim().to_string();
			let extrinsic = record[1].trim().to_string();
			map.entry(pallet).or_default().push(extrinsic);
		}
	}
	Ok(map)
}

/// Run command for pallet benchmarking.
///
/// # Arguments
/// * `cmd` - Command to benchmark the FRAME Pallets.
pub fn run_pallet_benchmarking(cmd: &PalletCmd) -> Result<()> {
	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| anyhow::anyhow!(format!("Failed to run benchmarking: {}", e.to_string())))
}

/// Performs a fuzzy search for pallets that match the provided input.
///
/// # Arguments
/// * `pallet_extrinsics` - A mapping of pallets and their extrinsics.
/// * `excluded_pallets` - Pallets that are excluded from the search results.
/// * `input` - The search input used to match pallets.
/// * `limit` - Maximum number of pallets returned from search.
pub fn search_for_pallets(
	pallet_extrinsics: &PalletExtrinsicsCollection,
	excluded_pallets: &Vec<String>,
	input: &str,
	limit: usize,
) -> Vec<String> {
	let pallets = pallet_extrinsics.keys();

	if input.is_empty() {
		return pallets.map(String::from).take(limit).collect();
	}
	let inputs = input.split(",");
	let pallets: Vec<&str> = pallets
		.map(String::as_str)
		.filter(|s| !excluded_pallets.contains(&s.to_string()))
		.collect();
	let mut output = inputs
		.flat_map(|input| fuzzy_search_best_n(input, &pallets, limit))
		.map(|v| v.0.to_string())
		.collect::<Vec<String>>();
	output.dedup();
	output
}

/// Performs a fuzzy search for extrinsics that match the provided input.
///
/// # Arguments
/// * `pallet_extrinsics` - A mapping of pallets and their extrinsics.
/// * `pallets` - List of pallets used to find the extrinsics.
/// * `input` - The search input used to match extrinsics.
pub fn search_for_extrinsics(
	pallet_extrinsics: &PalletExtrinsicsCollection,
	pallets: Vec<String>,
	input: &str,
	limit: usize,
) -> Vec<String> {
	let extrinsics: Vec<&str> = pallet_extrinsics
		.iter()
		.filter(|(pallet, _)| pallets.contains(pallet))
		.flat_map(|(_, extrinsics)| extrinsics.iter().map(String::as_str))
		.collect();

	if input.is_empty() {
		return extrinsics.into_iter().map(String::from).take(limit).collect();
	}
	let inputs = input.split(",");
	let mut output = inputs
		.flat_map(|input| fuzzy_search_best_n(input, &extrinsics, limit))
		.map(|v| v.0.to_string())
		.collect::<Vec<String>>();
	output.dedup();
	output
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[test]
	fn get_preset_names_works() -> anyhow::Result<()> {
		let runtime_path = std::env::current_dir()
			.unwrap()
			.join("../../tests/runtimes/base_parachain_benchmark.wasm")
			.canonicalize()?;
		assert_eq!(get_preset_names(&runtime_path)?, vec!["development", "local_testnet"]);
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

	#[test]
	fn list_pallets_and_extrinsics_works() -> Result<()> {
		let runtime_path = std::env::current_dir()
			.unwrap()
			.join("../../tests/runtimes/base_parachain_benchmark.wasm")
			.canonicalize()
			.unwrap();

		let pallets = list_pallets_and_extrinsics(&runtime_path)?;
		assert_eq!(
			pallets.get("pallet_timestamp").cloned().unwrap_or_default(),
			["on_finalize", "set"]
		);
		assert_eq!(
			pallets.get("pallet_sudo").cloned().unwrap_or_default(),
			["check_only_sudo_account", "remove_key", "set_key", "sudo", "sudo_as"]
		);
		Ok(())
	}

	#[test]
	fn parse_genesis_builder_policy_works() -> anyhow::Result<()> {
		for policy in ["none", "runtime"] {
			parse_genesis_builder_policy(policy)?;
		}
		Ok(())
	}
}
