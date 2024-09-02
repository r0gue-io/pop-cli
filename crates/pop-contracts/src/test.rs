// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use duct::cmd;
use std::{env, path::Path};

/// Run unit tests of a smart contract.
///
/// # Arguments
///
/// * `path` - location of the smart contract.
pub fn test_smart_contract(path: Option<&Path>) -> Result<(), Error> {
	// Execute `cargo test` command in the specified directory.
	cmd("cargo", vec!["test"])
		.dir(path.unwrap_or_else(|| Path::new("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

/// Run e2e tests of a smart contract.
///
/// # Arguments
///
/// * `path` - location of the smart contract.
/// * `node` - location of the contracts node binary.
pub fn test_e2e_smart_contract(path: Option<&Path>, node: Option<&Path>) -> Result<(), Error> {
	// Set the environment variable `CONTRACTS_NODE` to the path of the contracts node.
	if let Some(node) = node {
		env::set_var("CONTRACTS_NODE", node);
	}
	// Execute `cargo test --features=e2e-tests` command in the specified directory.
	cmd("cargo", vec!["test", "--features=e2e-tests"])
		.dir(path.unwrap_or_else(|| Path::new("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile;

	#[test]
	fn test_smart_contract_works() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
		// Run unit tests for the smart contract in the temporary contract directory.
		test_smart_contract(Some(&temp_dir.path().join("test_contract")))?;
		Ok(())
	}

	#[test]
	fn test_smart_contract_wrong_directory() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		assert!(matches!(
			test_smart_contract(Some(&temp_dir.path().join(""))),
			Err(Error::TestCommand(..))
		));
		Ok(())
	}

	#[test]
	fn test_e2e_smart_contract_set_env_variable() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
		// Ignore 2e2 testing in this scenario, will fail. Only test if the environment variable
		// CONTRACTS_NODE is set.
		let err = test_e2e_smart_contract(Some(&temp_dir.path().join("test_contract")), None);
		assert!(err.is_err());
		// The environment variable `CONTRACTS_NODE` should not be set.
		assert!(env::var("CONTRACTS_NODE").is_err());
		let err = test_e2e_smart_contract(
			Some(&temp_dir.path().join("test_contract")),
			Some(&Path::new("/path/to/contracts-node")),
		);
		assert!(err.is_err());
		// The environment variable `CONTRACTS_NODE` should has been set.
		assert_eq!(
			env::var("CONTRACTS_NODE").unwrap(),
			Path::new("/path/to/contracts-node").display().to_string()
		);
		Ok(())
	}

	#[test]
	fn test_e2e_smart_contract_fails_no_e2e_tests() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
		assert!(matches!(
			test_e2e_smart_contract(Some(&temp_dir.path().join("test_contract")), None),
			Err(Error::TestCommand(..))
		));
		Ok(())
	}
}
