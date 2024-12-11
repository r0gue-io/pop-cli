// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, utils::canonicalized_path, Contract};
use anyhow::Result;
use contract_build::new_contract_project;
use heck::ToUpperCamelCase;
use pop_common::{extract_template_files, replace_in_file, templates::Template, Git};
use std::{
	collections::HashMap,
	path::{Path, PathBuf},
};
use url::Url;

/// Create a new smart contract.
///
/// # Arguments
///
/// * `name` - name for the smart contract to be created.
/// * `target` - location where the smart contract will be created.
/// * `template` - template to generate the contract from.
pub fn create_smart_contract(name: &str, target: &Path, template: &Contract) -> Result<()> {
	let canonicalized_path = canonicalized_path(target)?;
	// Create a new default contract project with the provided name in the parent directory.
	if matches!(template, Contract::Standard) {
		return create_standard_contract(name, canonicalized_path);
	}
	create_template_contract(name, canonicalized_path, template)
}

pub fn is_valid_contract_name(name: &str) -> Result<(), Error> {
	if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
		return Err(Error::InvalidName(
			"Contract names can only contain alphanumeric characters and underscores".to_owned(),
		));
	}
	if !name.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
		return Err(Error::InvalidName(
			"Contract names must begin with an alphabetical character".to_owned(),
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
	new_contract_project(name, Some(parent_path))
		// If an error occurs during the creation of the contract project,
		// convert it into a NewContract variant with a formatted error message.
		.map_err(|e| Error::NewContract(format!("{}", e)))?;
	Ok(())
}
fn create_template_contract(
	name: &str,
	canonicalized_path: PathBuf,
	template: &Contract,
) -> Result<()> {
	let template_repository = template.repository_url()?;
	// Clone the repository into the temporary directory.
	let temp_dir = ::tempfile::TempDir::new_in(std::env::temp_dir())?;
	Git::clone(&Url::parse(template_repository)?, temp_dir.path(), None)?;
	// Retrieve only the template contract files.
	if template == &Contract::PSP22 || template == &Contract::PSP34 {
		// Different template structure requires extracting different path
		extract_template_files("", temp_dir.path(), canonicalized_path.as_path(), None)?;
	} else {
		extract_template_files(
			template.as_ref(),
			temp_dir.path(),
			canonicalized_path.as_path(),
			Some(vec!["frontend".to_string()]),
		)?;
	}

	// Replace name of the contract.
	rename_contract(name, canonicalized_path, template)?;
	Ok(())
}

pub fn rename_contract(name: &str, path: PathBuf, template: &Contract) -> Result<()> {
	let template_name = template.to_string().to_lowercase();
	// Replace name in the Cargo.toml file.
	let mut file_path = path.join("Cargo.toml");
	let mut replacements_in_cargo = HashMap::new();
	replacements_in_cargo.insert(template_name.as_str(), name);
	replace_in_file(file_path, replacements_in_cargo)?;
	// Replace name in the lib.rs file.
	file_path = path.join("lib.rs");
	let name_in_camel_case = name.to_upper_camel_case();
	replace_in_file(
		file_path,
		HashMap::from([(template_name.as_str(), name), (template.name(), &name_in_camel_case)]),
	)?;
	// Replace name in the e2e_tests.rs file if exists.
	let e2e_tests = path.join("e2e_tests.rs");
	if e2e_tests.exists() {
		let name_in_camel_case = format!("\"{}\"", name.to_upper_camel_case());
		replace_in_file(
			e2e_tests,
			HashMap::from([(template_name.as_str(), name), (template.name(), &name_in_camel_case)]),
		)?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::{fs, io::Write};

	fn setup_test_environment(template: Contract) -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		create_smart_contract("test_contract", temp_contract_dir.as_path(), &template)?;
		Ok(temp_dir)
	}

	#[test]
	fn test_create_standard_smart_contract_success() -> Result<(), Error> {
		let temp_dir = setup_test_environment(Contract::Standard)?;
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
		let temp_dir = setup_test_environment(Contract::ERC20)?;
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

	fn generate_contract_directory() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let config = temp_dir.path().join("Cargo.toml");
		let mut config_file = fs::File::create(config.clone())?;
		writeln!(
			config_file,
			r#"
				[package]
				name = "erc20"
				version = "5.0.0"
				authors = ["R0GUE"]
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

		let e2e_code = temp_dir.path().join("e2e_tests.rs");
		let mut e2e_code_file = fs::File::create(e2e_code.clone())?;
		writeln!(
			e2e_code_file,
			r#"
				#[ink_e2e::test]
					let contract = client
						.instantiate("erc20", &ink_e2e::alice(), &mut constructor)
						.submit()
						.await
						.expect("erc20 instantiate failed");
			"#
		)?;
		Ok(temp_dir)
	}
	#[test]
	fn test_rename_contract() -> Result<(), Error> {
		let temp_dir = generate_contract_directory()?;
		rename_contract("my_contract", temp_dir.path().to_owned(), &Contract::ERC20)?;
		let generated_cargo =
			fs::read_to_string(temp_dir.path().join("Cargo.toml")).expect("Could not read file");
		assert!(generated_cargo.contains("name = \"my_contract\""));

		let generated_code =
			fs::read_to_string(temp_dir.path().join("lib.rs")).expect("Could not read file");
		assert!(generated_code.contains("mod my_contract"));
		let generated_e2e_code =
			fs::read_to_string(temp_dir.path().join("e2e_tests.rs")).expect("Could not read file");
		assert!(generated_e2e_code
			.contains(".instantiate(\"my_contract\", &ink_e2e::alice(), &mut constructor)"));

		Ok(())
	}
}
