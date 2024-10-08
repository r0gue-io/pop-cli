// SPDX-License-Identifier: GPL-3.0

use crate::{create_smart_contract, Contract};
use anyhow::Result;
use std::{fs, path::PathBuf};

pub fn generate_smart_contract_test_environment() -> Result<tempfile::TempDir> {
	let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
	let temp_contract_dir = temp_dir.path().join("testing");
	fs::create_dir(&temp_contract_dir)?;
	create_smart_contract("testing", temp_contract_dir.as_path(), &Contract::Standard)?;
	Ok(temp_dir)
}

// Function that mocks the build process generating the contract artifacts.
pub fn mock_build_process(
	temp_contract_dir: PathBuf,
	contract_file: PathBuf,
	metadata_file: PathBuf,
) -> Result<()> {
	// Create a target directory
	let target_contract_dir = temp_contract_dir.join("target");
	fs::create_dir(&target_contract_dir)?;
	fs::create_dir(target_contract_dir.join("ink"))?;
	// Copy a mocked testing.contract and testing.json files inside the target directory
	fs::copy(contract_file, target_contract_dir.join("ink/testing.contract"))?;
	fs::copy(metadata_file, target_contract_dir.join("ink/testing.json"))?;
	Ok(())
}
