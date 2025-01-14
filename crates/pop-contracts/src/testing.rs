// SPDX-License-Identifier: GPL-3.0

use crate::{create_smart_contract, Contract};
use anyhow::Result;
use std::{
	fs::{copy, create_dir},
	path::Path,
};

/// Generates a smart contract test environment.
///
/// * `name` - The name of the contract to be created.
pub fn new_environment(name: &str) -> Result<tempfile::TempDir> {
	let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
	let temp_contract_dir = temp_dir.path().join(name);
	create_dir(&temp_contract_dir)?;
	create_smart_contract(name, temp_contract_dir.as_path(), &Contract::Standard)?;
	Ok(temp_dir)
}

/// Mocks the build process by generating contract artifacts in a specified temporary directory.
///
/// * `temp_contract_dir` - The root directory where the `target` folder and artifacts will be
///   created.
/// * `contract_file` - The path to the mocked contract file to be copied.
/// * `metadata_file` - The path to the mocked metadata file to be copied.
pub fn mock_build_process<P>(temp_contract_dir: P, contract_file: P, metadata_file: P) -> Result<()>
where
	P: AsRef<Path>,
{
	// Create a target directory
	let target_contract_dir = temp_contract_dir.as_ref().join("target");
	create_dir(&target_contract_dir)?;
	create_dir(target_contract_dir.join("ink"))?;
	// Copy a mocked testing.contract and testing.json files inside the target directory
	copy(contract_file, target_contract_dir.join("ink/testing.contract"))?;
	copy(metadata_file, target_contract_dir.join("ink/testing.json"))?;
	Ok(())
}
