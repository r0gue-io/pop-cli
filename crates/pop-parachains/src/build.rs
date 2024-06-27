// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use anyhow::Result;
use duct::cmd;
use pop_common::replace_in_file;
use serde_json::Value;
use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
};
use toml_edit::DocumentMut;

/// Build the parachain.
///
/// # Arguments
///
/// * `path` - Location of the parachain project.
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
pub fn binary_path(path: Option<&Path>) -> Result<String, Error> {
	let node_name = parse_node_name(path)?;
	let release_path = path.unwrap_or(Path::new("./")).join("target/release");
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
/// * `path` - Location of the parachain project.
/// * `para_id` - The parachain ID to be replaced in the specification.
pub fn generate_chain_spec(path: Option<&Path>, para_id: u32) -> Result<String, Error> {
	let parachain_folder = path.unwrap_or(Path::new("./"));
	let binary_path = binary_path(path)?;
	let plain_parachain_spec =
		format!("{}/plain-parachain-chainspec.json", parachain_folder.display());
	cmd(&binary_path, vec!["build-spec", "--disable-default-bootnode"])
		.stdout_path(plain_parachain_spec.clone())
		.run()?;
	let generated_para_id = get_parachain_id(&plain_parachain_spec)?;
	replace_para_id(
		parachain_folder.join("plain-parachain-chainspec.json"),
		para_id,
		generated_para_id,
	)?;
	let raw_chain_spec = format!("{}/raw-parachain-chainspec.json", parachain_folder.display());
	cmd(
		&binary_path,
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
/// * `chain_spec` - A `String` representing the path to the raw chain specification file.
/// * `path` - Location of the parachain project.
/// * `para_id` - The parachain ID will be used to name the wasm file.
pub fn export_wasm_file(
	chain_spec: &String,
	path: Option<&Path>,
	para_id: u32,
) -> Result<String, Error> {
	let parachain_folder = path.unwrap_or(Path::new("./"));
	let binary_path = binary_path(path)?;
	let wasm_file = format!("{}/para-{}-wasm", parachain_folder.display(), para_id);
	cmd(binary_path, vec!["export-genesis-wasm", "--chain", &chain_spec, &wasm_file]).run()?;
	Ok(wasm_file)
}

/// Generate the parachain genesis state.
///
/// # Arguments
///
/// * `chain_spec` - A `String` representing the path to the raw chain specification file.
/// * `path` - Location of the parachain project.
/// * `para_id` - The parachain ID will be used to name the wasm file.
pub fn generate_genesis_state_file(
	chain_spec: &String,
	path: Option<&Path>,
	para_id: u32,
) -> Result<String, Error> {
	let parachain_folder = path.unwrap_or(Path::new("./"));
	let binary_path = binary_path(path)?;
	let wasm_file = format!("{}/para-{}-genesis-state", parachain_folder.display(), para_id);
	cmd(binary_path.clone(), vec!["export-genesis-state", "--chain", &chain_spec, &wasm_file])
		.run()?;
	Ok(wasm_file)
}

/// Parses the node name from the Cargo.toml file located in the project path.
fn parse_node_name(path: Option<&Path>) -> Result<String, Error> {
	let cargo_toml = path.unwrap_or(Path::new("./")).join("node/Cargo.toml");
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

/// Get the current parachain id from the generated chain specification file.
fn get_parachain_id(plain_parachain_spec: &String) -> Result<u32> {
	let data = fs::read_to_string(plain_parachain_spec)?;
	let value = serde_json::from_str::<Value>(&data)?;
	Ok(value.get("para_id").and_then(Value::as_u64).unwrap_or(1000) as u32)
}

/// Replaces the generated parachain id in the chain specification file with the provided para_id.
fn replace_para_id(parachain_folder: PathBuf, para_id: u32, generated_para_id: u32) -> Result<()> {
	let mut replacements_in_cargo: HashMap<&str, &str> = HashMap::new();
	let old_para_id = format!("\"para_id\": {generated_para_id}");
	let new_para_id = format!("\"para_id\": {para_id}");
	replacements_in_cargo.insert(&old_para_id, &new_para_id);
	let old_parachain_id = format!("\"parachainId\": {generated_para_id}");
	let new_parachain_id = format!("\"parachainId\": {para_id}");
	replacements_in_cargo.insert(&old_parachain_id, &new_parachain_id);
	replace_in_file(parachain_folder, replacements_in_cargo)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		new_parachain::instantiate_standard_template, Config, ParachainTemplate as Template,
		Zombienet,
	};
	use anyhow::Result;
	use std::{fs, fs::metadata, io::Write, os::unix::fs::PermissionsExt, path::Path};
	use tempfile::{tempdir, Builder};

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

	// Function that fetch a binary from pop network
	async fn fetch_binary(cache: &Path) -> Result<String, Error> {
		let config = Builder::new().suffix(".toml").tempfile()?;
		writeln!(
			config.as_file(),
			r#"
[relaychain]
chain = "rococo-local"

[[parachains]]
id = 4385
default_command = "pop-node"
"#
		)?;
		let mut zombienet =
			Zombienet::new(&cache, config.path().to_str().unwrap(), None, None, None, None, None)
				.await?;
		let mut binary_name: String = "".to_string();
		for binary in zombienet.binaries().filter(|b| !b.exists() && b.name() == "pop-node") {
			binary_name = format!("{}-{}", binary.name(), binary.latest().unwrap());
			binary.source(true, &(), true).await?;
		}
		Ok(binary_name)
	}

	// Replace the binary fetched with the mocked binary
	fn replace_mock_with_binary(temp_dir: &Path, binary_name: String) -> Result<(), Error> {
		let content = fs::read(temp_dir.join(binary_name))?;
		fs::write(temp_dir.join("target/release/parachain-template-node"), content)?;
		// Make executable
		let mut perms =
			metadata(temp_dir.join("target/release/parachain-template-node"))?.permissions();
		perms.set_mode(0o755);
		std::fs::set_permissions(temp_dir.join("target/release/parachain-template-node"), perms)?;
		Ok(())
	}

	#[test]
	fn build_parachain_works() -> Result<()> {
		let temp_dir = tempdir()?;
		let name = "parachain_template_node";
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		build_parachain(&Some(PathBuf::from(temp_dir.path().join(name))))?;

		let target_folder = temp_dir.path().join(name).join("target/release");
		assert!(target_folder.exists());
		assert!(target_folder.join("parachain_template_node").exists());
		Ok(())
	}

	#[test]
	fn binary_path_works() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		mock_build_process(temp_dir.path())?;
		let release_path = binary_path(Some(Path::new(temp_dir.path())))?;
		assert_eq!(
			release_path,
			format!("{}/target/release/parachain-template-node", temp_dir.path().display())
		);
		Ok(())
	}

	#[test]
	fn binary_path_fails_missing_binary() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		assert!(matches!(
			binary_path(Some(Path::new(temp_dir.path()))),
			Err(Error::MissingBinary(error)) if error == "parachain-template-node"
		));
		Ok(())
	}

	#[tokio::test]
	async fn generate_files_works() -> anyhow::Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		mock_build_process(temp_dir.path())?;
		let binary_name = fetch_binary(temp_dir.path()).await?;
		replace_mock_with_binary(temp_dir.path(), binary_name)?;
		// Test generate chain spec
		let chain_spec = generate_chain_spec(Some(temp_dir.path()), 2001)?;
		let chain_spec_path = Path::new(&chain_spec);
		assert!(chain_spec_path.exists());
		let content = fs::read_to_string(chain_spec_path).expect("Could not read file");
		assert!(content.contains("\"para_id\": 2001"));
		// Test export wasm file
		let wasm_file = export_wasm_file(&chain_spec, Some(temp_dir.path()), 2001)?;
		assert!(Path::new(&wasm_file).exists());
		// Test generate parachain state file
		let genesis_file = generate_genesis_state_file(&chain_spec, Some(temp_dir.path()), 2001)?;
		assert!(Path::new(&genesis_file).exists());
		Ok(())
	}

	#[test]
	fn parse_node_name_works() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		let name = parse_node_name(Some(Path::new(temp_dir.path())))?;
		assert_eq!(name, "parachain-template-node");
		Ok(())
	}

	#[test]
	fn parse_node_name_node_cargo_no_exist() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		assert!(matches!(parse_node_name(Some(Path::new(temp_dir.path()))), Err(Error::IO(..))));
		Ok(())
	}

	#[test]
	fn parse_node_name_node_error_parsing_cargo() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		fs::create_dir(temp_dir.path().join("node"))?;
		let mut cargo_file = fs::File::create(temp_dir.path().join("node/Cargo.toml"))?;
		writeln!(cargo_file, "[")?;
		assert!(matches!(
			parse_node_name(Some(Path::new(temp_dir.path()))),
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
			parse_node_name(Some(Path::new(temp_dir.path()))),
			Err(Error::Config(error)) if error == "expected `name`",
		));
		Ok(())
	}

	#[test]
	fn get_parachain_id_works() -> Result<()> {
		let mut file = tempfile::NamedTempFile::new()?;
		writeln!(file, r#"{{ "name": "Local Testnet", "para_id": 2002 }}"#)?;
		let get_parachain_id = get_parachain_id(&file.path().display().to_string())?;
		assert_eq!(get_parachain_id, 2002);
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
		replace_para_id(file_path.clone(), 2001, 1000)?;
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
