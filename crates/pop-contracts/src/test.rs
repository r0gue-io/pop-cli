// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use duct::cmd;
use std::{env, path::Path};

/// Run e2e tests of a smart contract.
///
/// # Arguments
///
/// * `path` - location of the smart contract.
/// * `node` - location of the contracts node binary.
pub fn test_e2e_smart_contract(
	path: &Path,
	node: Option<&Path>,
	maybe_test_filter: Option<String>,
) -> Result<(), Error> {
	// Set the environment variable `CONTRACTS_NODE` to the path of the contracts node.
	if let Some(node) = node {
		unsafe {
			env::set_var("CONTRACTS_NODE", node);
		}
	}
	// Execute `cargo test --features=e2e-tests` command in the specified directory.
	let mut args = vec!["test".to_string(), "--features=e2e-tests".to_string()];
	if let Some(test_filter) = maybe_test_filter {
		args.push(test_filter);
	}
	cmd("cargo", args)
		.dir(path)
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_e2e_smart_contract_set_env_variable() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
		// Ignore 2e2 testing in this scenario, will fail. Only test if the environment variable
		// CONTRACTS_NODE is set.
		let err = test_e2e_smart_contract(&temp_dir.path().join("test_contract"), None, None);
		assert!(err.is_err());
		// The environment variable `CONTRACTS_NODE` should not be set.
		assert!(env::var("CONTRACTS_NODE").is_err());
		let err = test_e2e_smart_contract(
			&temp_dir.path().join("test_contract"),
			Some(Path::new("/path/to/contracts-node")),
			None,
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
			test_e2e_smart_contract(&temp_dir.path().join("test_contract"), None, None),
			Err(Error::TestCommand(..))
		));
		Ok(())
	}
}
