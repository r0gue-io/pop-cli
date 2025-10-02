// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, utils::get_manifest_path};
use contract_build::{execute, BuildMode, BuildResult, ExecuteArgs};
pub use contract_build::{MetadataSpec, Verbosity};
use std::path::Path;

/// Build the smart contract located at the specified `path` in `build_release` mode.
///
/// # Arguments
/// * `path` - The optional path to the smart contract manifest, defaulting to the current directory
///   if not specified.
/// * `release` - Whether the smart contract should be built without any debugging functionality.
/// * `verbosity` - The build output verbosity.
/// * `metadata_spec` - Optionally specify the contract metadata format/version.
pub fn build_smart_contract(
	path: Option<&Path>,
	release: bool,
	verbosity: Verbosity,
	metadata_spec: Option<MetadataSpec>,
) -> anyhow::Result<BuildResult> {
	let manifest_path = get_manifest_path(path)?;

	let build_mode = match release {
		true => BuildMode::Release,
		false => BuildMode::Debug,
	};

	let args =
		ExecuteArgs { manifest_path, build_mode, verbosity, metadata_spec, ..Default::default() };
	// Execute the build and log the output of the build
	execute(args)
}

/// Determines whether the manifest at the supplied path is a supported smart contract project.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not
///   specified.
pub fn is_supported(path: Option<&Path>) -> Result<bool, Error> {
	Ok(pop_common::manifest::from_path(path)?.dependencies.contains_key("ink"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use contract_build::new_contract_project;
	use duct::cmd;

	#[test]
	fn is_supported_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(path).run()?;
		assert!(!is_supported(Some(&path.join(name)))?);

		// Contract
		let name = "flipper";
		new_contract_project(name, Some(&path), None)?;
		assert!(is_supported(Some(&path.join(name)))?);
		Ok(())
	}
}
