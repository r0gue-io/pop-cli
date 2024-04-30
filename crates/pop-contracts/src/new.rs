// SPDX-License-Identifier: GPL-3.0
use crate::errors::Error;
use contract_build::new_contract_project;
use std::path::Path;

pub fn create_smart_contract(name: &str, target: &Path) -> Result<(), Error> {
	// Canonicalize the target path to ensure consistency and resolve any symbolic links.
	let canonicalized_path = target
		.canonicalize()
		// If an I/O error occurs during canonicalization, convert it into an Error enum variant.
		.map_err(|e| Error::IO(e))?;

	// Retrieve the parent directory of the canonicalized path.
	let parent_path = canonicalized_path
		.parent()
		// If the parent directory cannot be retrieved (e.g., if the path has no parent),
		// return a NewContract variant indicating the failure.
		.ok_or(Error::NewContract("Failed to get parent directory".to_string()))?;

	// Create a new contract project with the provided name in the parent directory.
	new_contract_project(&name, Some(parent_path))
		// If an error occurs during the creation of the contract project,
		// convert it into a NewContract variant with a formatted error message.
		.map_err(|e| Error::NewContract(format!("{}", e)))
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::fs;
	use tempfile;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		create_smart_contract("test_contract", temp_contract_dir.as_path())?;
		Ok(temp_dir)
	}

	#[test]
	fn test_create_smart_contract_success() -> Result<(), Error> {
		let temp_dir = setup_test_environment()?;

		// Verify that the generated smart contract contains the expected content
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("test_contract/lib.rs"))
				.expect("Could not read file");

		assert!(generated_file_content.contains("#[ink::contract]"));
		assert!(generated_file_content.contains("mod test_contract {"));

		// Verify that the generated Cargo.toml file contains the expected content
		fs::read_to_string(temp_dir.path().join("test_contract/Cargo.toml"))
			.expect("Could not read file");

		Ok(())
	}
}
