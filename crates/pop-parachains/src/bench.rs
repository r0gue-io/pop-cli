use anyhow::Result;
use frame_benchmarking_cli::PalletCmd;
use sp_runtime::traits::BlakeTwo256;
use std::path::PathBuf;
use subwasmlib::{source::Source, OutputFormat, Subwasm};
use subxt::ext::{
	codec::Decode,
	frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed},
};

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

/// List pallets and extrinsics.
///
/// # Arguments
/// * `runtime_path` - Path to the runtime WASM binary.
pub fn list_pallets_and_extrinsics(runtime_path: &PathBuf) -> Result<String> {
	let source = Source::from_options(Some(runtime_path.clone()), None, None, None)?;
	let subwasm = Subwasm::new(&source.try_into()?)?;
	let mut out = std::io::Cursor::new(Vec::new());
	subwasm.write_metadata(OutputFormat::Scale, None, &mut out)?;
	let output = out.into_inner();
	let decoded = RuntimeMetadataPrefixed::decode(&mut &output[..]).unwrap();

	match decoded.1 {
		RuntimeMetadata::V14(metadata) => {
			for pallet in metadata.pallets {
				let (index, name, calls) = (pallet.index, pallet.name, pallet.calls.unwrap());
			}
			Ok(())
		},
		RuntimeMetadata::V15(metadata) => {
			for pallet in metadata.pallets {
				let (index, name, calls) = (pallet.index, pallet.name, pallet.calls);
			}

			Ok(())
		},
		_ =>
			Err(anyhow::anyhow!("Unsupported Runtime version. This feature supports V12 and above")),
	}?;

	Ok(String::default())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::env;

	#[test]
	fn list_pallets_and_extrinsics_works() -> Result<()> {
		let runtime_path = env::current_dir()
			.unwrap()
			.join("../../tests/runtimes/base_parachain_benchmark.wasm")
			.canonicalize()
			.unwrap();

		list_pallets_and_extrinsics(&runtime_path)?;

		Ok(())
	}
}
