use anyhow::Result;
use clap::Parser;
use csv::Reader;
use frame_benchmarking_cli::PalletCmd;
use rust_fuzzy_search::fuzzy_search_sorted;
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

type HostFunctions = (
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

/// Run command for pallet benchmarking.
///
/// # Arguments
/// * `cmd` - Command to benchmark the FRAME Pallets.
pub fn run_pallet_benchmarking(cmd: &PalletCmd) -> Result<()> {
	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| anyhow::anyhow!(format!("Failed to run benchmarking: {}", e.to_string())))
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

/// List a mapping of pallets and their extrinsics.
///
/// # Arguments
/// * `runtime_path` - Path to the runtime WASM binary.
pub fn list_pallets_and_extrinsics(
	runtime_path: &Path,
) -> anyhow::Result<HashMap<String, Vec<String>>> {
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

/// Performs a fuzzy search for pallets that match the provided input.
///
/// # Arguments
/// * `pallet_extrinsics` - A mapping of pallets and their extrinsics.
/// * `input` - The search input used to match pallets.
pub fn search_for_pallets(
	pallet_extrinsics: &HashMap<String, Vec<String>>,
	input: &str,
) -> Vec<String> {
	let pallets = pallet_extrinsics.keys();

	if input.is_empty() {
		return pallets.map(String::from).collect();
	}
	let inputs = input.split(",");
	let pallets: Vec<&str> = pallets.map(|s| s.as_str()).collect();
	let mut output = inputs
		.flat_map(|input| fuzzy_search_sorted(input, &pallets))
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
	pallet_extrinsics: &HashMap<String, Vec<String>>,
	pallets: Vec<String>,
	input: &str,
) -> Vec<String> {
	let extrinsics: Vec<&str> = pallet_extrinsics
		.iter()
		.filter(|(pallet, _)| pallets.contains(pallet))
		.flat_map(|(_, extrinsics)| extrinsics.iter().map(String::as_str))
		.collect();

	if input.is_empty() {
		return extrinsics.into_iter().map(String::from).collect();
	}
	let inputs = input.split(",");
	let mut output = inputs
		.flat_map(|input| fuzzy_search_sorted(input, &extrinsics))
		.map(|v| v.0.to_string())
		.collect::<Vec<String>>();
	output.dedup();
	output
}

/// Check if a runtime has a genesis config preset.
///
/// # Arguments
/// * `binary_path` - Path to the runtime WASM binary.
/// * `preset` - Optional ID of the genesis config preset. If not provided, it checks the default
///   preset.
pub fn check_preset(binary_path: &PathBuf, preset: Option<&String>) -> anyhow::Result<()> {
	let binary = fs::read(binary_path).expect("No runtime binary found");
	let genesis_config_builder = GenesisConfigBuilderRuntimeCaller::<HostFunctions>::new(&binary);
	if genesis_config_builder.get_named_preset(preset).is_err() {
		return Err(anyhow::anyhow!(format!(
			r#"The preset with name "{:?}" is not available."#,
			preset
		)))
	}
	Ok(())
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

fn parse_csv_to_map(file_path: &PathBuf) -> anyhow::Result<HashMap<String, Vec<String>>> {
	let file = File::open(file_path)?;
	let mut rdr = Reader::from_reader(BufReader::new(file));
	let mut map: HashMap<String, Vec<String>> = HashMap::new();
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

#[cfg(test)]
mod tests {
	use super::*;

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
