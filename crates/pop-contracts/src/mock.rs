// SPDX-License-Identifier: GPL-3.0
use contract_build::{
	BuildResult, ExecuteArgs, MetadataArtifacts, OptimizationResult, OutputType, Verbosity,
};
use std::path::PathBuf;

// Mock the call to the `execute` function of the `contract_build' crate that builds the contract
pub fn execute(_args: ExecuteArgs) -> anyhow::Result<BuildResult> {
	Ok(BuildResult {
		dest_wasm: Some(PathBuf::from("/path/to/contract.wasm")),
		metadata_result: Some(MetadataArtifacts {
			dest_metadata: PathBuf::from("/path/to/contract.json"),
			dest_bundle: PathBuf::from("/path/to/contract.contract"),
		}),
		target_directory: PathBuf::from("/path/to/target"),
		optimization_result: Some(OptimizationResult { original_size: 64.0, optimized_size: 32.0 }),
		build_mode: Default::default(),
		build_artifact: Default::default(),
		image: None,
		verbosity: Verbosity::Quiet,
		output_type: OutputType::Json,
	})
}
