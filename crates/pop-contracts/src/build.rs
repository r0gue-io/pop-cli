use contract_build::{execute, ExecuteArgs, ManifestPath};
use std::path::PathBuf;

fn get_manifest_path(path: &Option<PathBuf>) -> anyhow::Result<ManifestPath> {
	if path.is_some() {
		let full_path: PathBuf =
			PathBuf::from(path.as_ref().unwrap().to_string_lossy().to_string() + "/Cargo.toml");

		return ManifestPath::try_from(Some(full_path));
	} else {
		return ManifestPath::try_from(path.as_ref());
	}
}

pub fn build_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<String> {
	// If the user specifies a path (which is not the current directory), it will have to manually
	// add a Cargo.toml file. If not provided, pop-cli will ask the user for a specific path. or ask
	// to the user the specific path (Like cargo-contract does)
	let manifest_path = get_manifest_path(path)?;
	// Default values
	let args = ExecuteArgs { manifest_path, ..Default::default() };

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

	#[test]
	fn test_get_manifest_path() -> Result<(), Error> {
		let temp_dir = setup_test_environment()?;
		let manifest_path =
			get_manifest_path(&Some(PathBuf::from(temp_dir.path().join("test_contract"))));
		assert!(manifest_path.is_ok());
		Ok(())
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
