use crate::errors::Error;
use duct::cmd;
use std::path::PathBuf;

pub fn test_smart_contract(path: &Option<PathBuf>) -> Result<(), Error> {
	// Execute `cargo test` command in the specified directory.
	let result = cmd("cargo", vec!["test"])
		.dir(path.clone().unwrap_or_else(|| PathBuf::from("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;

	if result.status.success() {
		Ok(())
	} else {
		Err(Error::TestCommand("Cargo test command failed.".to_string()))
	}
}

pub fn test_e2e_smart_contract(path: &Option<PathBuf>) -> Result<(), Error> {
	// Execute `cargo test --features=e2e-tests` command in the specified directory.
	let result = cmd("cargo", vec!["test", "--features=e2e-tests"])
		.dir(path.clone().unwrap_or_else(|| PathBuf::from("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;

	if result.status.success() {
		Ok(())
	} else {
		Err(Error::TestCommand("Cargo test command failed.".to_string()))
	}
}

#[cfg(feature = "unit_contract")]
#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempfile;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().map_err(|e| {
			Error::TestEnvironmentError(format!("Failed to create temp dir: {}", e))
		})?;
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir).map_err(|e| {
			Error::TestEnvironmentError(format!("Failed to create test contract directory: {}", e))
		})?;
		let result =
			crate::create_smart_contract("test_contract".to_string(), temp_contract_dir.as_path())
				.map_err(|e| {
					Error::TestEnvironmentError(format!("Failed to create smart contract: {}", e))
				})?;
		assert!(result.is_ok(), "Contract test environment setup failed");
		Ok(temp_dir)
	}

	#[test]
	fn test_contract_test() -> Result<(), Error> {
		let temp_contract_dir = setup_test_environment()?;
		// Run unit tests for the smart contract in the temporary contract directory.
		let result = test_smart_contract(&Some(temp_contract_dir.path().join("test_contract")))?;
		assert!(result.is_ok(), "Result should be Ok");
		Ok(())
	}
}
