// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use duct::cmd;
use std::{
	collections::HashMap,
	fs,
	io::{Read, Write},
	path::PathBuf,
};
use toml_edit::DocumentMut;

/// Build the parachain located in the specified `path`.
pub fn build_parachain(path: &Option<PathBuf>) -> Result<(), Error> {
	cmd("cargo", vec!["build", "--release"])
		.dir(path.clone().unwrap_or("./".into()))
		.run()?;

	Ok(())
}

/// Get the path to the node release binary based on the project path.
///
/// # Arguments
///
/// * `path` - Location of the parachain project.
pub fn node_release_path(path: &Option<PathBuf>) -> Result<String, Error> {
	let node_name = parse_node_name(path)?;
	let release_path = path.clone().unwrap_or("./".into()).join("target/release");
	let release = release_path.join(node_name.clone());
	if !release.exists() {
		return Err(Error::MissingBinary(node_name));
	}
	Ok(release.display().to_string())
}

/// Generates a raw chain specification file for a parachain.
///
/// # Arguments
///
/// * `binary_path` - A `String` representing the path to the binary used to build the specification.
/// * `path` - Location of the parachain project.
/// * `para_id` - The parachain ID to be replaced in the specification.
///
pub fn generate_chain_spec(
	binary_path: &String,
	path: &Option<PathBuf>,
	para_id: u32,
) -> Result<String, Error> {
	let parachain_folder = path.clone().unwrap_or("./".into());
	let plain_parachain_spec =
		format!("{}/plain-parachain-chainspec.json", parachain_folder.display());
	cmd(binary_path.clone(), vec!["build-spec", "--disable-default-bootnode"])
		.stdout_path(plain_parachain_spec.clone())
		.run()?;
	replace_para_id(parachain_folder.join("plain-parachain-chainspec.json"), para_id)?;
	let raw_chain_spec = format!("{}/raw-parachain-chainspec.json", parachain_folder.display());
	cmd(
		binary_path,
		vec!["build-spec", "--chain", &plain_parachain_spec, "--disable-default-bootnode", "--raw"],
	)
	.stdout_path(raw_chain_spec.clone())
	.run()?;
	Ok(raw_chain_spec)
}

/// Export the WebAssembly runtime for the parachain.
///
/// # Arguments
///
/// * `binary_path` - A `String` representing the path to the binary used to build the specification.
/// * `chain_spec` - A `String` representing the path to the raw chain specification file.
/// * `path` - Location of the parachain project.
/// * `para_id` - The parachain ID will be used to name the wasm file.
///
pub fn export_wasm_file(
	binary_path: &String,
	chain_spec: &String,
	path: &Option<PathBuf>,
	para_id: u32,
) -> Result<String, Error> {
	let parachain_folder = path.clone().unwrap_or("./".into());
	let wasm_file = format!("{}/para-{}-wasm", parachain_folder.display(), para_id);
	cmd(binary_path.clone(), vec!["export-genesis-wasm", "--chain", &chain_spec, &wasm_file])
		.run()?;
	Ok(wasm_file)
}

/// Generate the parachain genesis state.
///
/// # Arguments
///
/// * `binary_path` - A `String` representing the path to the binary used to build the specification.
/// * `chain_spec` - A `String` representing the path to the raw chain specification file.
/// * `path` - Location of the parachain project.
/// * `para_id` - The parachain ID will be used to name the wasm file.
///
pub fn generate_genesis_state_file(
	binary_path: &String,
	chain_spec: &String,
	path: &Option<PathBuf>,
	para_id: u32,
) -> Result<String, Error> {
	let parachain_folder = path.clone().unwrap_or("./".into());
	let wasm_file = format!("{}/para-{}-genesis-state", parachain_folder.display(), para_id);
	cmd(binary_path.clone(), vec!["export-genesis-state", "--chain", &chain_spec, &wasm_file])
		.run()?;
	Ok(wasm_file)
}

/// Parses the node name from the Cargo.toml file located in the project path.
fn parse_node_name(path: &Option<PathBuf>) -> Result<String, Error> {
	let cargo_toml = path.clone().unwrap_or("./".into()).join("node/Cargo.toml");
	let contents = std::fs::read_to_string(&cargo_toml)?;
	let config = contents.parse::<DocumentMut>().map_err(|err| Error::TomlError(err.into()))?;
	let name = config
		.get("package")
		.and_then(|i| i.as_table())
		.and_then(|t| t.get("name"))
		.and_then(|i| i.as_str())
		.ok_or_else(|| Error::Config("expected `name`".into()))?;
	Ok(name.to_string())
}

/// Replaces the generated parachain id in the chain specification file with the provided para_id.
fn replace_para_id(parachain_folder: PathBuf, para_id: u32) -> Result<(), Error> {
	let mut replacements_in_cargo: HashMap<&str, &str> = HashMap::new();
	let new_para_id = format!("\"para_id\": {para_id}");
	replacements_in_cargo.insert("\"para_id\": 1000", &new_para_id);
	let new_parachain_id = format!("\"parachainId\": {para_id}");
	replacements_in_cargo.insert("\"parachainId\": 1000", &new_parachain_id);
	replace_in_file(parachain_folder, replacements_in_cargo)?;
	Ok(())
}

// TODO: Use from common_crate in this PR: https://github.com/r0gue-io/pop-cli/pull/201/files when merged
fn replace_in_file(file_path: PathBuf, replacements: HashMap<&str, &str>) -> Result<(), Error> {
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{new_parachain::instantiate_standard_template, Config, Template};
	use anyhow::Result;
	use std::{fs, io::Write, path::Path};

	fn setup_template_and_instantiate() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		instantiate_standard_template(&Template::Standard, temp_dir.path(), config, None)?;
		Ok(temp_dir)
	}

	// Function that mocks the build process generating the target dir and release.
	fn mock_build_process(temp_dir: &Path) -> Result<(), Error> {
		// Create a target directory
		let target_dir = temp_dir.join("target");
		fs::create_dir(&target_dir)?;
		fs::create_dir(&target_dir.join("release"))?;
		// Create a release file
		fs::File::create(target_dir.join("release/parachain-template-node"))?;
		Ok(())
	}

	#[test]
	fn node_release_path_works() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		mock_build_process(temp_dir.path())?;
		let release_path = node_release_path(&Some(PathBuf::from(temp_dir.path())))?;
		assert_eq!(
			release_path,
			format!("{}/target/release/parachain-template-node", temp_dir.path().display())
		);
		Ok(())
	}

	#[test]
	fn node_release_path_fails_missing_binary() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		assert!(matches!(
			node_release_path(&Some(PathBuf::from(temp_dir.path()))),
			Err(Error::MissingBinary(error)) if error == "parachain-template-node"
		));
		Ok(())
	}

	#[test]
	fn parse_node_name_works() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		let name = parse_node_name(&Some(PathBuf::from(temp_dir.path())))?;
		assert_eq!(name, "parachain-template-node");
		Ok(())
	}

	#[test]
	fn parse_node_name_node_cargo_no_exist() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		assert!(matches!(
			parse_node_name(&Some(PathBuf::from(temp_dir.path()))),
			Err(Error::IO(..))
		));
		Ok(())
	}

	#[test]
	fn parse_node_name_node_error_parsing_cargo() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		fs::create_dir(temp_dir.path().join("node"))?;
		let mut cargo_file = fs::File::create(temp_dir.path().join("node/Cargo.toml"))?;
		writeln!(cargo_file, "[")?;
		assert!(matches!(
			parse_node_name(&Some(PathBuf::from(temp_dir.path()))),
			Err(Error::TomlError(..))
		));
		Ok(())
	}

	#[test]
	fn parse_node_name_node_error_parsing_name() -> Result<()> {
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
			parse_node_name(&Some(PathBuf::from(temp_dir.path()))),
			Err(Error::Config(error)) if error == "expected `name`",
		));
		Ok(())
	}

	#[test]
	fn replace_para_id_works() -> Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let file_path = temp_dir.path().join("chain-spec.json");
		let mut file = fs::File::create(temp_dir.path().join("chain-spec.json"))?;
		writeln!(
			file,
			r#"
				"name": "Local Testnet",
				"para_id": 1000,
				"parachainInfo": {{
					"parachainId": 1000
				}},
			"#
		)?;
		replace_para_id(file_path.clone(), 2001)?;
		let content = fs::read_to_string(file_path).expect("Could not read file");
		assert_eq!(
			content.trim(),
			r#"
				"name": "Local Testnet",
				"para_id": 2001,
				"parachainInfo": {
					"parachainId": 2001
				},
			"#
			.trim()
		);
		Ok(())
	}
}
