// SPDX-License-Identifier: GPL-3.0

use contract_build::{execute, BuildMode, ExecuteArgs};
use std::path::Path;

use crate::utils::helpers::get_manifest_path;

/// Build the smart contract located at the specified `path` in `build_release` mode.
pub fn build_smart_contract(path: Option<&Path>, build_release: bool) -> anyhow::Result<String> {
	let manifest_path = get_manifest_path(path)?;

	let build_mode = match build_release {
		true => BuildMode::Release,
		false => BuildMode::Debug,
	};
	// Default values
	let args = ExecuteArgs { manifest_path, build_mode, ..Default::default() };

	// Execute the build and log the output of the build
	let result = execute(args)?;
	let formatted_result = result.display();

	Ok(formatted_result)
}

/// Determines whether the manifest at the supplied path relates is a smart contract project.
pub fn is_smart_contract(path: Option<&Path>) -> bool {
	let Ok(manifest) = get_manifest_path(path) else {
		return false;
	};
	// Very simply check for now, can be improved by reading the manifest toml and iterating over the dependencies
	match std::fs::read_to_string(manifest) {
		Ok(contents) => contents.contains("ink ="),
		Err(_) => false,
	}
}
