use crate::utils::helpers::canonicalized_path;
// SPDX-License-Identifier: GPL-3.0
use crate::{errors::Error, utils::git::Git, Template};
use anyhow::Result;
use contract_build::new_contract_project;
use std::fs;
use std::path::{Path, PathBuf};

/// Create a new smart contract.
///
/// # Arguments
///
/// * `name` - name for the smart contract to be created.
/// * `target` - location where the smart contract will be created.
/// * `template` - template to generate the contract from.
pub fn create_smart_contract(name: &str, target: &Path, template: &Template) -> Result<()> {
	let canonicalized_path = canonicalized_path(target)?;
	// Create a new default contract project with the provided name in the parent directory.
	if matches!(template, Template::Flipper) {
		return create_flipper_contract(name, canonicalized_path);
	}
	return create_template_contract(canonicalized_path, &template);
}

fn create_flipper_contract(name: &str, canonicalized_path: PathBuf) -> Result<()> {
	let parent_path = canonicalized_path
		.parent()
		// If the parent directory cannot be retrieved (e.g., if the path has no parent),
		// return a NewContract variant indicating the failure.
		.ok_or(Error::NewContract("Failed to get parent directory".to_string()))?;
	new_contract_project(&name, Some(parent_path))
		// If an error occurs during the creation of the contract project,
		// convert it into a NewContract variant with a formatted error message.
		.map_err(|e| Error::NewContract(format!("{}", e)))?;
	return Ok(());
}
fn create_template_contract(canonicalized_path: PathBuf, template: &Template) -> Result<()> {
	let template_repository = template.repository_url()?;
	// Clone the repository into the temporary directory.
	let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
	Git::clone(&template_repository, temp_dir.path())?;
	// Retrieve only the template contract files.
	extract_contract_files(template.to_string(), temp_dir.path(), canonicalized_path.as_path())?;

	Ok(())
}

fn extract_contract_files(
	contract_name: String,
	repo_folder: &Path,
	target_folder: &Path,
) -> Result<()> {
	let contract_folder = repo_folder.join(contract_name);
	for entry in fs::read_dir(&contract_folder)? {
		let entry = entry?;
		// The currently available templates contain only files. The `frontend` folder is ignored.
		// If future templates include folders, functionality will need to be added to support copying directories as well.
		if entry.path().is_file() {
			fs::copy(entry.path(), target_folder.join(entry.file_name()))?;
		}
	}
	Ok(())
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
		create_smart_contract(
			"test_contract",
			temp_contract_dir.as_path(),
			&crate::Template::Flipper,
		)?;
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
