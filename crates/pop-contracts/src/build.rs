use contract_build::{
	execute, BuildArtifacts, BuildMode, ExecuteArgs, Features, ManifestPath, Network,
	OptimizationPasses, OutputType, Target, UnstableFlags, Verbosity, DEFAULT_MAX_MEMORY_PAGES,
};
use std::path::PathBuf;

pub fn build_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<String> {
	// If the user specifies a path (which is not the current directory), it will have to manually
	// add a Cargo.toml file. If not provided, pop-cli will ask the user for a specific path. or ask
	// to the user the specific path (Like cargo-contract does)
	let manifest_path;
	if path.is_some() {
		let full_path: PathBuf =
			PathBuf::from(path.as_ref().unwrap().to_string_lossy().to_string() + "/Cargo.toml");
		manifest_path = ManifestPath::try_from(Some(full_path))?;
	} else {
		manifest_path = ManifestPath::try_from(path.as_ref())?;
	}
	let args = ExecuteArgs {
		manifest_path,
		verbosity: Verbosity::Default,
		build_mode: BuildMode::Release,
		features: Features::default(),
		network: Network::Online,
		build_artifact: BuildArtifacts::All,
		unstable_flags: UnstableFlags::default(),
		optimization_passes: Some(OptimizationPasses::default()),
		keep_debug_symbols: false,
		extra_lints: false,
		output_type: OutputType::Json,
		skip_wasm_validation: false,
		target: Target::Wasm,
		max_memory_pages: DEFAULT_MAX_MEMORY_PAGES,
		image: Default::default(),
	};

	// Execute the build and log the output of the build
	let result = execute(args)?;
	let formatted_result = result.display();

	Ok(formatted_result)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::fs;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		let result =
			crate::create_smart_contract("test_contract".to_string(), temp_contract_dir.as_path());
		assert!(result.is_ok(), "Contract test environment setup failed");

		Ok(temp_dir)
	}

	#[cfg(feature = "unit_contract")]
	#[test]
	fn test_contract_build() -> Result<(), Error> {
		let temp_contract_dir = setup_test_environment()?;

		let build = build_smart_contract(&Some(temp_contract_dir.path().join("test_contract")));
		assert!(build.is_ok(), "Result should be Ok");

		// Verify that the folder target has been created
		assert!(temp_contract_dir.path().join("test_contract/target").exists());
		// Verify that all the artifacts has been generated
		assert!(temp_contract_dir
			.path()
			.join("test_contract/target/ink/test_contract.contract")
			.exists());
		assert!(temp_contract_dir
			.path()
			.join("test_contract/target/ink/test_contract.wasm")
			.exists());
		assert!(temp_contract_dir
			.path()
			.join("test_contract/target/ink/test_contract.json")
			.exists());

		Ok(())
	}
}
