use std::{collections::HashMap, fs::File};

use anyhow::Result;
use clap::Parser;
use csv::Reader;
use frame_benchmarking_cli::PalletCmd;
use sp_runtime::traits::BlakeTwo256;
use std::io::BufReader;
use tempfile::tempdir;

type HostFunctions = (
	sp_statement_store::runtime_api::HostFunctions,
	cumulus_primitives_proof_size_hostfunction::storage_proof_size::HostFunctions,
);

/// Generate benchmarks for a pallet.
///
/// # Arguments
/// * `cmd` - Command to benchmark the extrinsic weight of FRAME Pallets.
pub fn generate_benchmarks(cmd: &PalletCmd) -> Result<()> {
	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| anyhow::anyhow!(format!("Failed to run benchmarking: {}", e.to_string())))
}

/// Parse the pallet command from string value of genesis policy builder.
///
/// # Arguments
/// * `policy` - Genesis builder policy ( none | spec | runtime ).
pub fn parse_genesis_builder_policy(policy: &str) -> anyhow::Result<PalletCmd> {
	PalletCmd::try_parse_from(["", "--list", "--genesis-builder", policy])
		.map_err(|_| anyhow::anyhow!(format!("Invalid genesis builder option: {policy}")))
}

/// List pallets and extrinsics.
///
/// # Arguments
/// * `runtime_path` - Path to the runtime WASM binary.
pub fn list_pallets_and_extrinsics(runtime_path: &str) -> Result<HashMap<String, Vec<String>>> {
	let temp_dir = tempdir()?;
	let temp_file_path = temp_dir.path().join("pallets.csv");
	let cmd = PalletCmd::try_parse_from(&[
		"",
		"--runtime",
		runtime_path,
		&format!("--list={}", temp_file_path.to_str().unwrap()),
	])?;
	cmd.run_with_spec::<BlakeTwo256, HostFunctions>(None)
		.map_err(|e| anyhow::anyhow!(format!("Failed to list pallets: {}", e.to_string())))?;
	parse_csv_to_map(temp_file_path.to_str().unwrap())
}

fn parse_csv_to_map(file_path: &str) -> Result<HashMap<String, Vec<String>>> {
	let file = File::open(file_path)?;
	let mut rdr = Reader::from_reader(BufReader::new(file));
	let mut map: HashMap<String, Vec<String>> = HashMap::new();
	for result in rdr.records() {
		let record = result?;
		if record.len() == 2 {
			let pallet = record[0].trim().to_string();
			let extrinsic = record[1].trim().to_string();

			map.entry(pallet).or_insert_with(Vec::new).push(extrinsic);
		}
	}
	Ok(map)
}

#[cfg(test)]
mod tests {
	use super::*;
	use pop_common::find_project_root;

	#[test]
	fn list_pallets_and_extrinsics_works() -> Result<()> {
		let runtime_path = find_project_root().unwrap().join("tests/runtimes/base_parachain.wasm");
		let pallets = list_pallets_and_extrinsics(runtime_path.to_str().unwrap())?;
		println!("{:?}", pallets);
		Ok(())
	}
}
