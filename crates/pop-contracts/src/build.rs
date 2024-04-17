use contract_build::{execute, ExecuteArgs};
use std::path::PathBuf;
use thiserror::Error;

use crate::utils::helpers::get_manifest_path;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Failed to build smart contract: {0}")]
	BuildError(String),
	#[error("Contract test environment setup failed: {0}")]
	SetupError(String),
}

pub fn build_smart_contract(path: &Option<PathBuf>) -> Result<String, Error> {
	let manifest_path = match get_manifest_path(path) {
		Ok(path) => path,
		Err(e) => return Err(Error::BuildError(format!("Failed to get manifest path: {}", e))),
	};

	let args = ExecuteArgs { manifest_path, ..Default::default() };
	let result = execute(args).map_err(|e| Error::BuildError(format!("{}", e)))?;
	let formatted_result = result.display();
	Ok(formatted_result)
}

#[cfg(feature = "unit_contract")]
#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::fs;

	fn setup_test_environment() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir()?;
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		crate::create_smart_contract("test_contract".to_string(), temp_contract_dir.as_path())?;
		Ok(temp_dir)
	}

	#[test]
	fn test_contract_build() -> Result<()> {
		let temp_contract_dir = setup_test_environment()?;
		let build = build_smart_contract(&Some(temp_contract_dir.path().join("test_contract")))?;
		assert!(build.is_ok(), "Result should be Ok");

		assert!(temp_contract_dir.path().join("test_contract/target").exists());
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
