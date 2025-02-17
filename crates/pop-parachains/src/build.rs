// SPDX-License-Identifier: GPL-3.0

use crate::Error::{self, *};
use anyhow::{anyhow, Result};
use duct::cmd;
use pop_common::{manifest::from_path, Profile};
use serde_json::{json, Value};
use std::{
	fs,
	path::{Path, PathBuf},
	str::FromStr,
};

/// Build the parachain and returns the path to the binary.
///
/// # Arguments
/// * `path` - The optional path to the parachain manifest, defaulting to the current directory if
///   not specified.
/// * `package` - The optional package to be built.
/// * `profile` - Whether the parachain should be built without any debugging functionality.
/// * `node_path` - An optional path to the node directory. Defaults to the `node` subdirectory of
///   the project path if not provided.
pub fn build_parachain(
	path: &Path,
	package: Option<String>,
	profile: &Profile,
	node_path: Option<&Path>,
) -> Result<PathBuf, Error> {
	let mut args = vec!["build"];
	if let Some(package) = package.as_deref() {
		args.push("--package");
		args.push(package)
	}
	if profile == &Profile::Release {
		args.push("--release");
	} else if profile == &Profile::Production {
		args.push("--profile=production");
	}
	cmd("cargo", args).dir(path).run()?;
	binary_path(&profile.target_directory(path), node_path.unwrap_or(&path.join("node")))
}

/// Determines whether the manifest at the supplied path is a supported parachain project.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not
///   specified.
pub fn is_supported(path: Option<&Path>) -> Result<bool, Error> {
	let manifest = from_path(path)?;
	// Simply check for a parachain dependency
	const DEPENDENCIES: [&str; 4] =
		["cumulus-client-collator", "cumulus-primitives-core", "parachains-common", "polkadot-sdk"];
	Ok(DEPENDENCIES.into_iter().any(|d| {
		manifest.dependencies.contains_key(d) ||
			manifest.workspace.as_ref().map_or(false, |w| w.dependencies.contains_key(d))
	}))
}

/// Constructs the node binary path based on the target path and the node directory path.
///
/// # Arguments
/// * `target_path` - The path where the binaries are expected to be found.
/// * `node_path` - The path to the node from which the node name will be parsed.
pub fn binary_path(target_path: &Path, node_path: &Path) -> Result<PathBuf, Error> {
	let manifest = from_path(Some(node_path))?;
	let node_name = manifest.package().name();
	let release = target_path.join(node_name);
	if !release.exists() {
		return Err(Error::MissingBinary(node_name.to_string()));
	}
	Ok(release)
}

/// Generates the plain text chain specification for a parachain.
///
/// # Arguments
/// * `binary_path` - The path to the node binary executable that contains the `build-spec` command.
/// * `plain_chain_spec` - Location of the plain_parachain_spec file to be generated.
/// * `default_bootnode` - Whether to include localhost as a bootnode.
/// * `chain` - The chain specification. It can be one of the predefined ones (e.g. dev, local or a
///   custom one) or the path to an existing chain spec.
pub fn generate_plain_chain_spec(
	binary_path: &Path,
	plain_chain_spec: &Path,
	default_bootnode: bool,
	chain: &str,
) -> Result<(), Error> {
	check_command_exists(binary_path, "build-spec")?;
	let mut args = vec!["build-spec", "--chain", chain];
	if !default_bootnode {
		args.push("--disable-default-bootnode");
	}
	// Create a temporary file.
	let temp_file = tempfile::NamedTempFile::new_in(std::env::temp_dir())?;
	// Run the command and redirect output to the temporary file.
	cmd(binary_path, args).stdout_path(temp_file.path()).stderr_null().run()?;
	// Atomically replace the chain spec file with the temporary file.
	temp_file.persist(plain_chain_spec).map_err(|e| {
		AnyhowError(anyhow!(
			"Failed to replace the chain spec file with the temporary file: {}",
			e.to_string()
		))
	})?;
	Ok(())
}

/// Generates a raw chain specification file for a parachain.
///
/// # Arguments
/// * `binary_path` - The path to the node binary executable that contains the `build-spec` command.
/// * `plain_chain_spec` - Location of the plain chain specification file.
/// * `chain_spec_file_name` - The name of the chain specification file to be generated.
pub fn generate_raw_chain_spec(
	binary_path: &Path,
	plain_chain_spec: &Path,
	chain_spec_file_name: &str,
) -> Result<PathBuf, Error> {
	if !plain_chain_spec.exists() {
		return Err(Error::MissingChainSpec(plain_chain_spec.display().to_string()));
	}
	check_command_exists(binary_path, "build-spec")?;
	let raw_chain_spec = plain_chain_spec.with_file_name(chain_spec_file_name);
	cmd(
		binary_path,
		vec![
			"build-spec",
			"--chain",
			&plain_chain_spec.display().to_string(),
			"--disable-default-bootnode",
			"--raw",
		],
	)
	.stderr_null()
	.stdout_path(&raw_chain_spec)
	.run()?;
	Ok(raw_chain_spec)
}

/// Export the WebAssembly runtime for the parachain.
///
/// # Arguments
/// * `binary_path` - The path to the node binary executable that contains the `export-genesis-wasm`
///   command.
/// * `chain_spec` - Location of the raw chain specification file.
/// * `wasm_file_name` - The name of the wasm runtime file to be generated.
pub fn export_wasm_file(
	binary_path: &Path,
	chain_spec: &Path,
	wasm_file_name: &str,
) -> Result<PathBuf, Error> {
	if !chain_spec.exists() {
		return Err(Error::MissingChainSpec(chain_spec.display().to_string()));
	}
	check_command_exists(binary_path, "export-genesis-wasm")?;
	let wasm_file = chain_spec.parent().unwrap_or(Path::new("./")).join(wasm_file_name);
	cmd(
		binary_path,
		vec![
			"export-genesis-wasm",
			"--chain",
			&chain_spec.display().to_string(),
			&wasm_file.display().to_string(),
		],
	)
	.stdout_null()
	.stderr_null()
	.run()?;
	Ok(wasm_file)
}

/// Generate the parachain genesis state.
///
/// # Arguments
/// * `binary_path` - The path to the node binary executable that contains the
///   `export-genesis-state` command.
/// * `chain_spec` - Location of the raw chain specification file.
/// * `genesis_file_name` - The name of the genesis state file to be generated.
pub fn generate_genesis_state_file(
	binary_path: &Path,
	chain_spec: &Path,
	genesis_file_name: &str,
) -> Result<PathBuf, Error> {
	if !chain_spec.exists() {
		return Err(Error::MissingChainSpec(chain_spec.display().to_string()));
	}
	check_command_exists(binary_path, "export-genesis-state")?;
	let genesis_file = chain_spec.parent().unwrap_or(Path::new("./")).join(genesis_file_name);
	cmd(
		binary_path,
		vec![
			"export-genesis-state",
			"--chain",
			&chain_spec.display().to_string(),
			&genesis_file.display().to_string(),
		],
	)
	.stdout_null()
	.stderr_null()
	.run()?;
	Ok(genesis_file)
}

/// Checks if a given command exists and can be executed by running it with the "--help" argument.
fn check_command_exists(binary_path: &Path, command: &str) -> Result<(), Error> {
	cmd(binary_path, vec![command, "--help"]).stdout_null().run().map_err(|_err| {
		Error::MissingCommand {
			command: command.to_string(),
			binary: binary_path.display().to_string(),
		}
	})?;
	Ok(())
}

/// A chain specification.
pub struct ChainSpec(Value);
impl ChainSpec {
	/// Parses a chain specification from a path.
	///
	/// # Arguments
	/// * `path` - The path to a chain specification file.
	pub fn from(path: &Path) -> Result<ChainSpec> {
		Ok(ChainSpec(Value::from_str(&std::fs::read_to_string(path)?)?))
	}

	/// Get the chain type from the chain specification.
	pub fn get_chain_type(&self) -> Option<&str> {
		self.0.get("chainType").and_then(|v| v.as_str())
	}

	/// Get the parachain ID from the chain specification.
	pub fn get_parachain_id(&self) -> Option<u64> {
		self.0.get("para_id").and_then(|v| v.as_u64())
	}

	/// Get the protocol ID from the chain specification.
	pub fn get_protocol_id(&self) -> Option<&str> {
		self.0.get("protocolId").and_then(|v| v.as_str())
	}

	/// Get the relay chain from the chain specification.
	pub fn get_relay_chain(&self) -> Option<&str> {
		self.0.get("relay_chain").and_then(|v| v.as_str())
	}

	/// Replaces the parachain id with the provided `para_id`.
	///
	/// # Arguments
	/// * `para_id` - The new value for the para_id.
	pub fn replace_para_id(&mut self, para_id: u32) -> Result<(), Error> {
		// Replace para_id
		let replace = self
			.0
			.get_mut("para_id")
			.ok_or_else(|| Error::Config("expected `para_id`".into()))?;
		*replace = json!(para_id);

		// Replace genesis.runtimeGenesis.patch.parachainInfo.parachainId
		let replace = self
			.0
			.get_mut("genesis")
			.ok_or_else(|| Error::Config("expected `genesis`".into()))?
			.get_mut("runtimeGenesis")
			.ok_or_else(|| Error::Config("expected `runtimeGenesis`".into()))?
			.get_mut("patch")
			.ok_or_else(|| Error::Config("expected `patch`".into()))?
			.get_mut("parachainInfo")
			.ok_or_else(|| Error::Config("expected `parachainInfo`".into()))?
			.get_mut("parachainId")
			.ok_or_else(|| Error::Config("expected `parachainInfo.parachainId`".into()))?;
		*replace = json!(para_id);
		Ok(())
	}

	/// Replaces the relay chain name with the given one.
	///
	/// # Arguments
	/// * `relay_name` - The new value for the relay chain field in the specification.
	pub fn replace_relay_chain(&mut self, relay_name: &str) -> Result<(), Error> {
		// Replace relay_chain
		let replace = self
			.0
			.get_mut("relay_chain")
			.ok_or_else(|| Error::Config("expected `relay_chain`".into()))?;
		*replace = json!(relay_name);
		Ok(())
	}

	/// Replaces the chain type with the given one.
	///
	/// # Arguments
	/// * `chain_type` - The new value for the chain type.
	pub fn replace_chain_type(&mut self, chain_type: &str) -> Result<(), Error> {
		// Replace chainType
		let replace = self
			.0
			.get_mut("chainType")
			.ok_or_else(|| Error::Config("expected `chainType`".into()))?;
		*replace = json!(chain_type);
		Ok(())
	}

	/// Replaces the protocol ID with the given one.
	///
	/// # Arguments
	/// * `protocol_id` - The new value for the protocolId of the given specification.
	pub fn replace_protocol_id(&mut self, protocol_id: &str) -> Result<(), Error> {
		// Replace protocolId
		let replace = self
			.0
			.get_mut("protocolId")
			.ok_or_else(|| Error::Config("expected `protocolId`".into()))?;
		*replace = json!(protocol_id);
		Ok(())
	}

	/// Converts the chain specification to a string.
	pub fn to_string(&self) -> Result<String> {
		Ok(serde_json::to_string_pretty(&self.0)?)
	}

	/// Writes the chain specification to a file.
	///
	/// # Arguments
	/// * `path` - The path to the chain specification file.
	pub fn to_file(&self, path: &Path) -> Result<()> {
		fs::write(path, self.to_string()?)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		new_parachain::instantiate_standard_template, templates::Parachain, Config, Error,
		Zombienet,
	};
	use anyhow::Result;
	use pop_common::{manifest::Dependency, set_executable_permission};
	use std::{fs, fs::write, io::Write, path::Path};
	use strum::VariantArray;
	use tempfile::{tempdir, Builder, TempDir};

	fn setup_template_and_instantiate() -> Result<TempDir> {
		let temp_dir = tempdir().expect("Failed to create temp dir");
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		instantiate_standard_template(&Parachain::Standard, temp_dir.path(), config, None)?;
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

	// Function that generates a Cargo.toml inside node directory for testing.
	fn generate_mock_node(temp_dir: &Path, name: Option<&str>) -> Result<PathBuf, Error> {
		// Create a node directory
		let target_dir = temp_dir.join(name.unwrap_or("node"));
		fs::create_dir(&target_dir)?;
		// Create a Cargo.toml file
		let mut toml_file = fs::File::create(target_dir.join("Cargo.toml"))?;
		writeln!(
			toml_file,
			r#"
			[package]
			name = "parachain_template_node"
			version = "0.1.0"

			[dependencies]

			"#
		)?;
		Ok(target_dir)
	}

	// Function that fetch a binary from pop network
	async fn fetch_binary(cache: &Path) -> Result<String, Error> {
		let config = Builder::new().suffix(".toml").tempfile()?;
		writeln!(
			config.as_file(),
			r#"
            [relaychain]
            chain = "paseo-local"

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
	fn replace_mock_with_binary(temp_dir: &Path, binary_name: String) -> Result<PathBuf, Error> {
		let binary_path = temp_dir.join(binary_name);
		let content = fs::read(&binary_path)?;
		write(temp_dir.join("target/release/parachain-template-node"), content)?;
		// Make executable
		set_executable_permission(temp_dir.join("target/release/parachain-template-node"))?;
		Ok(binary_path)
	}

	fn add_production_profile(project: &Path) -> Result<()> {
		let root_toml_path = project.join("Cargo.toml");
		let mut root_toml_content = fs::read_to_string(&root_toml_path)?;
		root_toml_content.push_str(
			r#"
			[profile.production]
			codegen-units = 1
			inherits = "release"
			lto = true
			"#,
		);
		// Write the updated content back to the file
		write(&root_toml_path, root_toml_content)?;
		Ok(())
	}

	#[test]
	fn build_parachain_works() -> Result<()> {
		let name = "parachain_template_node";
		let temp_dir = tempdir()?;
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		let project = temp_dir.path().join(name);
		add_production_profile(&project)?;
		for node in vec![None, Some("custom_node")] {
			let node_path = generate_mock_node(&project, node)?;
			for package in vec![None, Some(String::from("parachain_template_node"))] {
				for profile in Profile::VARIANTS {
					let node_path = node.map(|_| node_path.as_path());
					let binary = build_parachain(&project, package.clone(), &profile, node_path)?;
					let target_directory = profile.target_directory(&project);
					assert!(target_directory.exists());
					assert!(target_directory.join("parachain_template_node").exists());
					assert_eq!(
						binary.display().to_string(),
						target_directory.join("parachain_template_node").display().to_string()
					);
				}
			}
		}
		Ok(())
	}

	#[test]
	fn binary_path_works() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		mock_build_process(temp_dir.path())?;
		let release_path =
			binary_path(&temp_dir.path().join("target/release"), &temp_dir.path().join("node"))?;
		assert_eq!(
			release_path.display().to_string(),
			format!("{}/target/release/parachain-template-node", temp_dir.path().display())
		);
		Ok(())
	}

	#[test]
	fn binary_path_fails_missing_binary() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		assert!(matches!(
			binary_path(&temp_dir.path().join("target/release"), &temp_dir.path().join("node")),
			Err(Error::MissingBinary(error)) if error == "parachain-template-node"
		));
		Ok(())
	}

	#[tokio::test]
	async fn generate_files_works() -> Result<()> {
		let temp_dir =
			setup_template_and_instantiate().expect("Failed to setup template and instantiate");
		mock_build_process(temp_dir.path())?;
		let binary_name = fetch_binary(temp_dir.path()).await?;
		let binary_path = replace_mock_with_binary(temp_dir.path(), binary_name)?;
		// Test generate chain spec
		let plain_chain_spec = &temp_dir.path().join("plain-parachain-chainspec.json");
		generate_plain_chain_spec(
			&binary_path,
			&temp_dir.path().join("plain-parachain-chainspec.json"),
			false,
			"local",
		)?;
		assert!(plain_chain_spec.exists());
		{
			let mut chain_spec = ChainSpec::from(&plain_chain_spec)?;
			chain_spec.replace_para_id(2001)?;
			chain_spec.to_file(&plain_chain_spec)?;
		}
		let raw_chain_spec = generate_raw_chain_spec(
			&binary_path,
			&plain_chain_spec,
			"raw-parachain-chainspec.json",
		)?;
		assert!(raw_chain_spec.exists());
		let content = fs::read_to_string(raw_chain_spec.clone()).expect("Could not read file");
		assert!(content.contains("\"para_id\": 2001"));
		assert!(content.contains("\"id\": \"pop-devnet\""));
		assert!(content.contains("\"bootNodes\": []"));
		// Test export wasm file
		let wasm_file = export_wasm_file(&binary_path, &raw_chain_spec, "para-2001-wasm")?;
		assert!(wasm_file.exists());
		// Test generate parachain state file
		let genesis_file =
			generate_genesis_state_file(&binary_path, &raw_chain_spec, "para-2001-genesis-state")?;
		assert!(genesis_file.exists());
		Ok(())
	}

	#[test]
	fn raw_chain_spec_fails_wrong_chain_spec() -> Result<()> {
		assert!(matches!(
			generate_raw_chain_spec(
				Path::new("./binary"),
				Path::new("./plain-parachain-chainspec.json"),
				"plain-parachain-chainspec.json"
			),
			Err(Error::MissingChainSpec(..))
		));
		Ok(())
	}

	#[test]
	fn export_wasm_file_fails_wrong_chain_spec() -> Result<()> {
		assert!(matches!(
			export_wasm_file(
				Path::new("./binary"),
				Path::new("./raw-parachain-chainspec"),
				"para-2001-wasm"
			),
			Err(Error::MissingChainSpec(..))
		));
		Ok(())
	}

	#[test]
	fn generate_genesis_state_file_wrong_chain_spec() -> Result<()> {
		assert!(matches!(
			generate_genesis_state_file(
				Path::new("./binary"),
				Path::new("./raw-parachain-chainspec"),
				"para-2001-genesis-state",
			),
			Err(Error::MissingChainSpec(..))
		));
		Ok(())
	}

	#[test]
	fn get_chain_type_works() -> Result<()> {
		let chain_spec = ChainSpec(json!({
			"chainType": "test",
		}));
		assert_eq!(chain_spec.get_chain_type(), Some("test"));
		Ok(())
	}

	#[test]
	fn get_parachain_id_works() -> Result<()> {
		let chain_spec = ChainSpec(json!({
			"para_id": 2002,
		}));
		assert_eq!(chain_spec.get_parachain_id(), Some(2002));
		Ok(())
	}

	#[test]
	fn get_protocol_id_works() -> Result<()> {
		let chain_spec = ChainSpec(json!({
			"protocolId": "test",
		}));
		assert_eq!(chain_spec.get_protocol_id(), Some("test"));
		Ok(())
	}

	#[test]
	fn get_relay_chain_works() -> Result<()> {
		let chain_spec = ChainSpec(json!({
			"relay_chain": "test",
		}));
		assert_eq!(chain_spec.get_relay_chain(), Some("test"));
		Ok(())
	}

	#[test]
	fn replace_para_id_works() -> Result<()> {
		let mut chain_spec = ChainSpec(json!({
			"para_id": 1000,
			"genesis": {
				"runtimeGenesis": {
					"patch": {
						"parachainInfo": {
							"parachainId": 1000
						}
					}
				}
			},
		}));
		chain_spec.replace_para_id(2001)?;
		assert_eq!(
			chain_spec.0,
			json!({
				"para_id": 2001,
				"genesis": {
					"runtimeGenesis": {
						"patch": {
							"parachainInfo": {
								"parachainId": 2001
							}
						}
					}
				},
			})
		);
		Ok(())
	}

	#[test]
	fn replace_para_id_fails() -> Result<()> {
		let mut chain_spec = ChainSpec(json!({
			"genesis": {
				"runtimeGenesis": {
					"patch": {
						"parachainInfo": {
							"parachainId": 1000
						}
					}
				}
			},
		}));
		assert!(
			matches!(chain_spec.replace_para_id(2001), Err(Error::Config(error)) if error == "expected `para_id`")
		);
		chain_spec = ChainSpec(json!({
			"para_id": 2001,
			"": {
				"runtimeGenesis": {
					"patch": {
						"parachainInfo": {
							"parachainId": 1000
						}
					}
				}
			},
		}));
		assert!(
			matches!(chain_spec.replace_para_id(2001), Err(Error::Config(error)) if error == "expected `genesis`")
		);
		chain_spec = ChainSpec(json!({
			"para_id": 2001,
			"genesis": {
				"": {
					"patch": {
						"parachainInfo": {
							"parachainId": 1000
						}
					}
				}
			},
		}));
		assert!(
			matches!(chain_spec.replace_para_id(2001), Err(Error::Config(error)) if error == "expected `runtimeGenesis`")
		);
		chain_spec = ChainSpec(json!({
			"para_id": 2001,
			"genesis": {
				"runtimeGenesis": {
					"": {
						"parachainInfo": {
							"parachainId": 1000
						}
					}
				}
			},
		}));
		assert!(
			matches!(chain_spec.replace_para_id(2001), Err(Error::Config(error)) if error == "expected `patch`")
		);
		chain_spec = ChainSpec(json!({
			"para_id": 2001,
			"genesis": {
				"runtimeGenesis": {
					"patch": {
						"": {
							"parachainId": 1000
						}
					}
				}
			},
		}));
		assert!(
			matches!(chain_spec.replace_para_id(2001), Err(Error::Config(error)) if error == "expected `parachainInfo`")
		);
		chain_spec = ChainSpec(json!({
			"para_id": 2001,
			"genesis": {
				"runtimeGenesis": {
					"patch": {
						"parachainInfo": {
						}
					}
				}
			},
		}));
		assert!(
			matches!(chain_spec.replace_para_id(2001), Err(Error::Config(error)) if error == "expected `parachainInfo.parachainId`")
		);
		Ok(())
	}

	#[test]
	fn replace_relay_chain_works() -> Result<()> {
		let mut chain_spec = ChainSpec(json!({"relay_chain": "old-relay"}));
		chain_spec.replace_relay_chain("new-relay")?;
		assert_eq!(chain_spec.0, json!({"relay_chain": "new-relay"}));
		Ok(())
	}

	#[test]
	fn replace_relay_chain_fails() -> Result<()> {
		let mut chain_spec = ChainSpec(json!({"": "old-relay"}));
		assert!(
			matches!(chain_spec.replace_relay_chain("new-relay"), Err(Error::Config(error)) if error == "expected `relay_chain`")
		);
		Ok(())
	}

	#[test]
	fn replace_chain_type_works() -> Result<()> {
		let mut chain_spec = ChainSpec(json!({"chainType": "old-chainType"}));
		chain_spec.replace_chain_type("new-chainType")?;
		assert_eq!(chain_spec.0, json!({"chainType": "new-chainType"}));
		Ok(())
	}

	#[test]
	fn replace_chain_type_fails() -> Result<()> {
		let mut chain_spec = ChainSpec(json!({"": "old-chainType"}));
		assert!(
			matches!(chain_spec.replace_chain_type("new-chainType"), Err(Error::Config(error)) if error == "expected `chainType`")
		);
		Ok(())
	}

	#[test]
	fn replace_protocol_id_works() -> Result<()> {
		let mut chain_spec = ChainSpec(json!({"protocolId": "old-protocolId"}));
		chain_spec.replace_protocol_id("new-protocolId")?;
		assert_eq!(chain_spec.0, json!({"protocolId": "new-protocolId"}));
		Ok(())
	}

	#[test]
	fn replace_protocol_id_fails() -> Result<()> {
		let mut chain_spec = ChainSpec(json!({"": "old-protocolId"}));
		assert!(
			matches!(chain_spec.replace_protocol_id("new-protocolId"), Err(Error::Config(error)) if error == "expected `protocolId`")
		);
		Ok(())
	}

	#[test]
	fn check_command_exists_fails() -> Result<()> {
		let binary_path = PathBuf::from("/bin");
		let cmd = "nonexistent_command";
		assert!(matches!(
			check_command_exists(&binary_path, cmd),
			Err(Error::MissingCommand {command, binary })
			if command == cmd && binary == binary_path.display().to_string()
		));
		Ok(())
	}

	#[test]
	fn is_supported_works() -> Result<()> {
		let temp_dir = tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(&path).run()?;
		assert!(!is_supported(Some(&path.join(name)))?);

		// Parachain
		let mut manifest = from_path(Some(&path.join(name)))?;
		manifest
			.dependencies
			.insert("cumulus-client-collator".into(), Dependency::Simple("^0.14.0".into()));
		let manifest = toml_edit::ser::to_string_pretty(&manifest)?;
		write(path.join(name).join("Cargo.toml"), manifest)?;
		assert!(is_supported(Some(&path.join(name)))?);
		Ok(())
	}
}
