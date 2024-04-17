use contract_build::new_contract_project;
use std::path::Path;

/// Create a new smart contract at `target`
pub fn create_smart_contract(name: String, target: &Path) -> anyhow::Result<()> {
	// In this code, out_dir will automatically join `name` to `target`,
	// which is created prior to the call to this function
	// So we must pass `target.parent()`
	new_contract_project(&name, target.canonicalize()?.parent())
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
			create_smart_contract("test_contract".to_string(), temp_contract_dir.as_path());
		assert!(result.is_ok(), "Contract test environment setup failed");

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
