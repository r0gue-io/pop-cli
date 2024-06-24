// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use duct::cmd;
use std::path::PathBuf;
use toml_edit::DocumentMut;

/// Build the parachain located in the specified `path`.
pub fn build_parachain(path: &Option<PathBuf>) -> Result<(), Error> {
	cmd("cargo", vec!["build", "--release"])
		.dir(path.clone().unwrap_or("./".into()))
		.run()?;

	Ok(())
}

pub fn node_release_path(path: &Option<PathBuf>) -> Result<String, Error> {
	let node_name = parse_node_name(path)?;
	let release_path = path.clone().unwrap_or("./".into()).join("target/release");
	let release = release_path.join(node_name.clone());
	if !release.exists() {
		return Err(Error::MissingBinary(node_name));
	}
	Ok(release.display().to_string())
}

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
}
