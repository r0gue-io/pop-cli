// SPDX-License-Identifier: GPL-3.0
use contract_build::{execute, BuildMode, ExecuteArgs};
use std::path::PathBuf;

use crate::utils::helpers::get_manifest_path;

/// Build the smart contract located in the specified `path` in `build_release` mode.
pub fn build_smart_contract(path: &Option<PathBuf>, build_release: bool) -> anyhow::Result<String> {
	let manifest_path = get_manifest_path(path)?;

	let build_mode = match build_release {
		true => BuildMode::Release,
		false => BuildMode::Debug,
	};
	println!("build_mode: {:?}", build_mode);
	// Default values
	let args = ExecuteArgs { manifest_path, build_mode, ..Default::default() };

	// Execute the build and log the output of the build
	let result = execute(args)?;
	let formatted_result = result.display();

	Ok(formatted_result)
}
