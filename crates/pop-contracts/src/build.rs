// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, utils::get_manifest_path};
use contract_build::{
	BuildMode, BuildResult, ComposeBuildArgs, ExecuteArgs, ManifestPath, execute,
};
pub use contract_build::{MetadataSpec, Verbosity};
use pop_common::manifest::Manifest;
use std::{fs, path::Path};
use toml::Value;

/// POST processing build arguments.
struct NoPostProcessing;

impl ComposeBuildArgs for NoPostProcessing {
	fn compose_build_args() -> anyhow::Result<Vec<String>> {
		Ok(vec![])
	}
}

/// Build the smart contract located at the specified `path` in `build_release` mode.
///
/// # Arguments
/// * `path` - The optional path to the smart contract manifest, defaulting to the current directory
///   if not specified.
/// * `release` - Whether the smart contract should be built without any debugging functionality.
/// * `verbosity` - The build output verbosity.
/// * `metadata_spec` - Optionally specify the contract metadata format/version.
pub fn build_smart_contract(
	path: &Path,
	release: bool,
	verbosity: Verbosity,
	metadata_spec: Option<MetadataSpec>,
) -> anyhow::Result<Vec<BuildResult>> {
	let manifest_path = get_manifest_path(path)?;

	let metadata_spec = match metadata_spec {
		s @ Some(_) => s,
		None => resolve_metadata_spec(manifest_path.as_ref())?,
	};

	let build_mode = match release {
		true => BuildMode::Release,
		false => BuildMode::Debug,
	};

	let mut manifest_paths = vec![];
	let manifest = Manifest::from_path(manifest_path.as_ref())?;
	if let Some(workspace) = manifest.workspace {
		for member in &workspace.members {
			let path = ManifestPath::new(path.join(member).join("Cargo.toml").as_path())?;
			if matches!(is_supported(path.as_ref()), Ok(true)) {
				manifest_paths.push(path);
			}
		}
		if manifest_paths.is_empty() {
			return Err(anyhow::anyhow!("Workspace must contain at least one ink! contract member"));
		}
	} else {
		manifest_paths.push(manifest_path);
	};

	// Perform a build for every contract in the workspace, or a single one if not in a workspace.
	manifest_paths
		.into_iter()
		.map(|manifest_path| {
			let args = ExecuteArgs {
				manifest_path,
				build_mode,
				verbosity,
				metadata_spec,
				..Default::default()
			};
			// Execute the build and log the output of the build
			execute::<NoPostProcessing>(args)
		})
		.collect()
}

/// Determine the metadata spec to use inferring from the
/// `[package.metadata.ink-lang]` `abi` setting in `Cargo.toml`.
fn resolve_metadata_spec(manifest_path: &Path) -> anyhow::Result<Option<MetadataSpec>> {
	let manifest_contents = fs::read_to_string(manifest_path)?;
	let manifest: Value = toml::from_str(&manifest_contents)?;

	let abi = manifest
		.get("package")
		.and_then(Value::as_table)
		.and_then(|pkg| pkg.get("metadata"))
		.and_then(Value::as_table)
		.and_then(|metadata| metadata.get("ink-lang"))
		.and_then(Value::as_table)
		.and_then(|ink_lang| ink_lang.get("abi"))
		.and_then(Value::as_str)
		.map(|abi| abi.to_lowercase());

	Ok(match abi.as_deref() {
		// Prefer Solidity metadata when the contract is configured for Solidity or dual ABI.
		Some("sol") | Some("all") => Some(MetadataSpec::Solidity),
		_ => None,
	})
}

/// Determines whether the manifest at the supplied path is a supported smart contract project.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not
///   specified.
pub fn is_supported(path: &Path) -> Result<bool, Error> {
	let manifest = pop_common::manifest::from_path(path)?;
	match manifest.workspace {
		Some(workspace) => Ok(workspace.dependencies.contains_key("ink")),
		None => Ok(manifest.dependencies.contains_key("ink")),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use contract_build::new_contract_project;
	use duct::cmd;
	use std::fs;

	#[test]
	fn is_supported_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(path).run()?;
		assert!(!is_supported(&path.join(name))?);

		// Contract
		let name = "flipper";
		new_contract_project(name, Some(&path), None)?;
		assert!(is_supported(&path.join(name))?);
		Ok(())
	}

	#[test]
	fn resolve_metadata_spec_infers_solidity_for_sol_abi() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let manifest_path = temp_dir.path().join("Cargo.toml");
		fs::write(
			&manifest_path,
			r#"[package]
name = "dummy"
version = "0.1.0"
[package.metadata.ink-lang]
abi = "sol"
"#,
		)?;

		let spec = resolve_metadata_spec(&manifest_path)?;
		assert_eq!(spec, Some(MetadataSpec::Solidity));
		Ok(())
	}

	#[test]
	fn resolve_metadata_spec_infers_solidity_for_all_abi() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let manifest_path = temp_dir.path().join("Cargo.toml");
		fs::write(
			&manifest_path,
			r#"[package]
name = "dummy"
version = "0.1.0"
[package.metadata.ink-lang]
abi = "all"
"#,
		)?;

		let spec = resolve_metadata_spec(&manifest_path)?;
		assert_eq!(spec, Some(MetadataSpec::Solidity));
		Ok(())
	}

	#[test]
	fn resolve_metadata_spec_handles_explicit_ink_abi() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let manifest_path = temp_dir.path().join("Cargo.toml");
		fs::write(
			&manifest_path,
			r#"[package]
name = "dummy"
version = "0.1.0"
[package.metadata.ink-lang]
abi = "ink"
"#,
		)?;

		let spec = resolve_metadata_spec(&manifest_path)?;
		assert!(spec.is_none());
		Ok(())
	}

	#[test]
	fn resolve_metadata_spec_defaults_for_ink() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let manifest_path = temp_dir.path().join("Cargo.toml");
		fs::write(
			&manifest_path,
			r#"[package]
name = "dummy"
version = "0.1.0"
"#,
		)?;

		let spec = resolve_metadata_spec(&manifest_path)?;
		assert!(spec.is_none());
		Ok(())
	}
}
