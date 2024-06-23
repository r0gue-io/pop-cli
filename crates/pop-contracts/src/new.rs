// SPDX-License-Identifier: GPL-3.0
use crate::{
	errors::Error,
	utils::helpers::{canonicalized_path, replace_in_file},
	Template,
};
use anyhow::Result;
use contract_build::new_contract_project;
use heck::ToUpperCamelCase;
use pop_common::Git;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use url::Url;

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
	if matches!(template, Template::Standard) {
		return create_standard_contract(name, canonicalized_path);
	}
	return create_template_contract(name, canonicalized_path, &template);
}

pub fn is_valid_contract_name(name: &str) -> Result<(), Error> {
	if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
		return Err(Error::InvalidName(
			"Contract names can only contain alphanumeric characters and underscores".to_owned(),
		));
	}
	if !name.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
		return Err(Error::InvalidName(
			"Contract names must begin with an alphabetic character".to_owned(),
		));
	}
	Ok(())
}

fn create_standard_contract(name: &str, canonicalized_path: PathBuf) -> Result<()> {
	let parent_path = canonicalized_path
		.parent()
		// If the parent directory cannot be retrieved (e.g. if the path has no parent),
		// return a NewContract variant indicating the failure.
		.ok_or(Error::NewContract("Failed to get parent directory".to_string()))?;
	new_contract_project(&name, Some(parent_path))
		// If an error occurs during the creation of the contract project,
		// convert it into a NewContract variant with a formatted error message.
		.map_err(|e| Error::NewContract(format!("{}", e)))?;
	Ok(())
}
fn create_template_contract(
	name: &str,
	canonicalized_path: PathBuf,
	template: &Template,
) -> Result<()> {
	let template_repository = template.repository_url()?;
	// Clone the repository into the temporary directory.
	let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
	Git::clone(&Url::parse(template_repository)?, temp_dir.path(), None)?;
	// Retrieve only the template contract files.
	extract_contract_files(template.to_string(), temp_dir.path(), canonicalized_path.as_path())?;
	// Replace name of the contract.
	rename_contract(name, canonicalized_path, template)?;
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
		// The currently available templates contain only files: lib.rs and Carg.toml. The `frontend` folder is being ignored.
		// If future templates include folders, functionality will need to be added to support copying directories as well.
		if entry.path().is_file() {
			fs::copy(entry.path(), target_folder.join(entry.file_name()))?;
		}
	}
	Ok(())
}

pub fn rename_contract(name: &str, path: PathBuf, template: &Template) -> Result<()> {
	let template_name = template.to_string().to_lowercase();
	// Replace name in the Cargo.toml file.
	let mut file_path = path.join("Cargo.toml");
	let mut replacements_in_cargo = HashMap::new();
	replacements_in_cargo.insert(template_name.as_str(), name);
	replace_in_file(file_path, replacements_in_cargo)?;

	// Replace name in the lib.rs file.
	file_path = path.join("lib.rs");
	let name_in_camel_case = name.to_upper_camel_case();
	let mut replacements_in_contract = HashMap::new();
	replacements_in_contract.insert(template_name.as_str(), name);
	replacements_in_contract.insert(template.name(), &name_in_camel_case);
	replace_in_file(file_path, replacements_in_contract)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::{fs, io::Write};
	use tempfile::{self};

	fn setup_test_environment(template: Template) -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		create_smart_contract("test_contract", temp_contract_dir.as_path(), &template)?;
		Ok(temp_dir)
	}

	#[test]
	fn test_create_standard_smart_contract_success() -> Result<(), Error> {
		let temp_dir = setup_test_environment(Template::Standard)?;

		// Verify that the generated smart contract contains the expected content
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("test_contract/lib.rs"))
				.expect("Could not read file");

		assert!(generated_file_content.contains("#[ink::contract]"));
		assert!(generated_file_content.contains("mod test_contract {"));
		assert!(generated_file_content.contains("pub struct TestContract {"));
		assert!(generated_file_content.contains("impl TestContract {"));

		// Verify that the generated Cargo.toml file contains the expected content
		let generated_cargo = fs::read_to_string(temp_dir.path().join("test_contract/Cargo.toml"))
			.expect("Could not read file");
		assert!(generated_cargo.contains("name = \"test_contract\""));

		Ok(())
	}

	#[test]
	fn test_create_template_smart_contract_success() -> Result<(), Error> {
		let temp_dir = setup_test_environment(Template::ERC20)?;

		// Verify that the generated smart contract contains the expected content
		let generated_file_content =
			fs::read_to_string(temp_dir.path().join("test_contract/lib.rs"))
				.expect("Could not read file");

		assert!(generated_file_content.contains("#[ink::contract]"));
		assert!(generated_file_content.contains("mod test_contract {"));
		assert!(generated_file_content.contains("pub struct TestContract {"));
		assert!(generated_file_content.contains("impl TestContract {"));

		// Verify that the generated Cargo.toml file contains the expected content
		let generated_cargo = fs::read_to_string(temp_dir.path().join("test_contract/Cargo.toml"))
			.expect("Could not read file");
		assert!(generated_cargo.contains("name = \"test_contract\""));

		Ok(())
	}

	#[test]
	fn test_is_valid_contract_name() -> Result<(), Error> {
		assert!(is_valid_contract_name("my_contract").is_ok());
		assert!(is_valid_contract_name("normal").is_ok());
		assert!(is_valid_contract_name("123").is_err());
		assert!(is_valid_contract_name("my-contract").is_err());
		assert!(is_valid_contract_name("contract**").is_err());
		Ok(())
	}

	fn generate_testing_files_and_folders(template: Template) -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let contract_folder = temp_dir.path().join(template.to_string());
		fs::create_dir(&contract_folder)?;
		fs::File::create(&contract_folder.join("lib.rs"))?;
		fs::File::create(&contract_folder.join("Cargo.toml"))?;
		fs::create_dir(&temp_dir.path().join("noise_folder"))?;
		Ok(temp_dir)
	}

	#[test]
	fn test_extract_contract_files() -> Result<(), Error> {
		// ERC-20
		let mut temp_dir = generate_testing_files_and_folders(Template::ERC20)?;
		let mut output_dir = tempfile::tempdir()?;
		extract_contract_files(Template::ERC20.to_string(), temp_dir.path(), output_dir.path())?;
		assert!(output_dir.path().join("lib.rs").exists());
		assert!(output_dir.path().join("Cargo.toml").exists());
		assert!(!output_dir.path().join("noise_folder").exists());
		// ERC-721
		temp_dir = generate_testing_files_and_folders(Template::ERC721)?;
		output_dir = tempfile::tempdir()?;
		extract_contract_files(Template::ERC721.to_string(), temp_dir.path(), output_dir.path())?;
		assert!(output_dir.path().join("lib.rs").exists());
		assert!(output_dir.path().join("Cargo.toml").exists());
		assert!(!output_dir.path().join("noise_folder").exists());
		// ERC-1155
		temp_dir = generate_testing_files_and_folders(Template::ERC1155)?;
		output_dir = tempfile::tempdir()?;
		extract_contract_files(Template::ERC1155.to_string(), temp_dir.path(), output_dir.path())?;
		assert!(output_dir.path().join("lib.rs").exists());
		assert!(output_dir.path().join("Cargo.toml").exists());
		assert!(!output_dir.path().join("noise_folder").exists());

		Ok(())
	}

	fn generate_contract_folder() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let config = temp_dir.path().join("Cargo.toml");
		let mut config_file = fs::File::create(config.clone())?;
		writeln!(
			config_file,
			r#"
				[package]
				name = "erc20"
				version = "5.0.0"
				authors = ["Parity Technologies <admin@parity.io>"]
				edition = "2021"
				publish = false
			"#
		)?;
		let code = temp_dir.path().join("lib.rs");
		let mut code_file = fs::File::create(code.clone())?;
		writeln!(
			code_file,
			r#"
				#[ink::contract]
				mod erc20
			"#
		)?;
		Ok(temp_dir)
	}
	#[test]
	fn test_rename_contract() -> Result<(), Error> {
		let temp_dir = generate_contract_folder()?;
		rename_contract("my_contract", temp_dir.path().to_owned(), &Template::ERC20)?;
		let generated_cargo =
			fs::read_to_string(temp_dir.path().join("Cargo.toml")).expect("Could not read file");
		assert!(generated_cargo.contains("name = \"my_contract\""));

		let generated_code =
			fs::read_to_string(temp_dir.path().join("lib.rs")).expect("Could not read file");
		assert!(generated_code.contains("mod my_contract"));

		Ok(())
	}
}
