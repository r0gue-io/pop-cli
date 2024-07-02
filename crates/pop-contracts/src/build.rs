// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, utils::helpers::get_manifest_path};
use contract_build::{execute, BuildMode, ExecuteArgs};
use std::{env, path::PathBuf};

/// Build the smart contract located at the specified `path` in `build_release` mode.
pub fn build_smart_contract(path: &Option<PathBuf>, build_release: bool) -> Result<String, Error> {
	let manifest_path = get_manifest_path(&path)?;
	// Set the CARGO_TARGET_DIR variable to this project target
	let target_path = manifest_path.absolute_directory()?.join("target");
	env::set_var("CARGO_TARGET_DIR", target_path.display().to_string());

	let build_mode = match build_release {
		true => BuildMode::Release,
		false => BuildMode::Debug,
	};
	// Default values
	let args = ExecuteArgs { manifest_path, build_mode, ..Default::default() };

	// Execute the build and log the output of the build
	let result =
		execute(args).map_err(|error| Error::BuildContractError(format!("{:?}", error)))?;
	let formatted_result = result.display();

	Ok(formatted_result)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use duct::cmd;
	use tempfile::tempdir;

	#[test]
	fn build_parachain_fails_no_ink_project() -> Result<()> {
		let temp_dir = tempdir()?;
		let name = "my_contract";
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		assert!(matches!(
			build_smart_contract(&Some(PathBuf::from(temp_dir.path().join(name))), false),
			Err(Error::BuildContractError(..))
		));
		// Assert env variable has been set
		assert_eq!(
			env::var("CARGO_TARGET_DIR")?,
			format!(
				"/private{}", // manifest_path.absolute_directory() adds a /private prefix
				PathBuf::from(temp_dir.path().join("my_contract/target")).display().to_string()
			)
		);
		Ok(())
	}
}
