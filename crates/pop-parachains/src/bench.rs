use anyhow::Result;
use clap::Parser;
use frame_benchmarking_cli::PalletCmd;
use sp_runtime::traits::BlakeTwo256;

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
