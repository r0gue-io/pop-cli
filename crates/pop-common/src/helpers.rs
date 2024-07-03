// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use std::{
	collections::HashMap,
	fs,
	io::{Read, Write},
	path::{Path, PathBuf},
};
use toml_edit::DocumentMut;

/// Replaces occurrences of specified strings in a file with new values.
///
/// # Arguments
///
/// * `file_path` - A `PathBuf` specifying the path to the file to be modified.
/// * `replacements` - A `HashMap` where each key-value pair represents
///   a target string and its corresponding replacement string.
///
pub fn replace_in_file(file_path: PathBuf, replacements: HashMap<&str, &str>) -> Result<(), Error> {
	// Read the file content
	let mut file_content = String::new();
	fs::File::open(&file_path)?.read_to_string(&mut file_content)?;
	// Perform the replacements
	let mut modified_content = file_content;
	for (target, replacement) in &replacements {
		modified_content = modified_content.replace(target, replacement);
	}
	// Write the modified content back to the file
	let mut file = fs::File::create(&file_path)?;
	file.write_all(modified_content.as_bytes())?;
	Ok(())
}

/// Parses the package name from the `Cargo.toml` file located in the specified node path.
///
/// # Arguments
/// * `node_path` - The path to the node directory containing the `Cargo.toml` file.
pub fn parse_package_name(node_path: &Path) -> Result<String, Error> {
	let manifest = node_path.join("Cargo.toml");
	let contents = std::fs::read_to_string(&manifest)?;
	let config = contents.parse::<DocumentMut>().map_err(|err| Error::TomlError(err.into()))?;
	let name = config
		.get("package")
		.and_then(|i| i.as_table())
		.and_then(|t| t.get("name"))
		.and_then(|i| i.as_str())
		.ok_or_else(|| Error::Config("expected `name`".into()))?;
	Ok(name.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::fs;

	// Function that generates a Cargo.toml inside node folder for testing.
	fn generate_mock_node(temp_dir: &Path) -> Result<PathBuf, Error> {
		// Create a node directory
		let target_dir = temp_dir.join("node");
		fs::create_dir(&target_dir).expect("Failed to create node directory");
		// Create a Cargo.toml file
		let mut toml_file =
			fs::File::create(target_dir.join("Cargo.toml")).expect("Failed to create Cargo.toml");
		writeln!(
			toml_file,
			r#"
			[package]
			name = "parachain-template-node"
			version = "0.1.0"
			authors.workspace = true
			edition.workspace = true
			homepage.workspace = true
			license.workspace = true
			repository.workspace = true

			[dependencies]

			"#
		)
		.expect("Failed to write to Cargo.toml");
		Ok(target_dir)
	}

	#[test]
	fn test_replace_in_file() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		let file_path = temp_dir.path().join("file.toml");
		let mut file = fs::File::create(temp_dir.path().join("file.toml"))?;
		writeln!(file, "name = test, version = 5.0.0")?;
		let mut replacements_in_cargo = HashMap::new();
		replacements_in_cargo.insert("test", "changed_name");
		replacements_in_cargo.insert("5.0.0", "5.0.1");
		replace_in_file(file_path.clone(), replacements_in_cargo)?;
		let content = fs::read_to_string(file_path).expect("Could not read file");
		assert_eq!(content.trim(), "name = changed_name, version = 5.0.1");
		Ok(())
	}

	#[test]
	fn parse_package_name_works() -> Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let node_path = generate_mock_node(temp_dir.path())?;
		let name = parse_package_name(&node_path)?;
		assert_eq!(name, "parachain-template-node");
		Ok(())
	}

	#[test]
	fn parse_package_name_node_cargo_no_exist() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		assert!(matches!(parse_package_name(&temp_dir.path().join("node")), Err(Error::IO(..))));
		Ok(())
	}

	#[test]
	fn parse_package_name_node_error_parsing_cargo() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		fs::create_dir(temp_dir.path().join("node"))?;
		let mut cargo_file = fs::File::create(temp_dir.path().join("node/Cargo.toml"))?;
		writeln!(cargo_file, "[")?;
		assert!(matches!(
			parse_package_name(&temp_dir.path().join("node")),
			Err(Error::TomlError(..))
		));
		Ok(())
	}

	#[test]
	fn parse_package_name_node_error_parsing_name() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		fs::create_dir(temp_dir.path().join("node"))?;
		let mut cargo_file = fs::File::create(temp_dir.path().join("node/Cargo.toml"))?;
		writeln!(
			cargo_file,
			r#"
				[package]
				version = "0.1.0"
			"#
		)?;
		assert!(matches!(
			parse_package_name(&temp_dir.path().join("node")),
			Err(Error::Config(error)) if error == "expected `name`",
		));
		Ok(())
	}
}
