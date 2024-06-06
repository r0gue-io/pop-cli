// SPDX-License-Identifier: GPL-3.0
use crate::errors::Error;
use duct::cmd;
use std::path::PathBuf;

/// Run unit tests of a smart contract.
///
/// # Arguments
///
/// * `path` - location of the smart contract.
pub fn test_smart_contract(path: &Option<PathBuf>) -> Result<(), Error> {
	// Execute `cargo test` command in the specified directory.
	cmd("cargo", vec!["test"])
		.dir(path.clone().unwrap_or_else(|| PathBuf::from("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

/// Run the e2e tests of a smart contract.
///
/// # Arguments
///
/// * `path` - location of the smart contract.
pub fn test_e2e_smart_contract(path: &Option<PathBuf>) -> Result<(), Error> {
	// Execute `cargo test --features=e2e-tests` command in the specified directory.
	cmd("cargo", vec!["test", "--features=e2e-tests"])
		.dir(path.clone().unwrap_or_else(|| PathBuf::from("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

#[cfg(feature = "unit_contract")]
#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use tempfile;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		crate::create_smart_contract(
			"test_contract",
			temp_contract_dir.as_path(),
			&crate::Template::Flipper,
		)?;
		Ok(temp_dir)
	}

	#[test]
	fn test_contract_test() -> Result<(), Error> {
		let temp_contract_dir = setup_test_environment()?;
		// Run unit tests for the smart contract in the temporary contract directory.
		test_smart_contract(&Some(temp_contract_dir.path().join("test_contract")))?;
		Ok(())
	}
}
