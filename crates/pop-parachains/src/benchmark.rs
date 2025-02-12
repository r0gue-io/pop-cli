use anyhow::Result;
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
