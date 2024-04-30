// SPDX-License-Identifier: GPL-3.0
use contract_build::{execute, ExecuteArgs};
use std::path::PathBuf;

use crate::utils::helpers::get_manifest_path;

pub fn build_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<String> {
	let manifest_path = get_manifest_path(path)?;
	// Default values
	let args = ExecuteArgs { manifest_path, ..Default::default() };

	// Execute the build and log the output of the build
	let result = execute(args)?;
	let formatted_result = result.display();

	Ok(formatted_result)
}

#[cfg(feature = "unit_contract")]
#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::fs;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		let result = crate::create_smart_contract("test_contract", temp_contract_dir.as_path());
		assert!(result.is_ok(), "Contract test environment setup failed");

		Ok(temp_dir)
	}

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
