// SPDX-License-Identifier: GPL-3.0
use contract_build::{execute, ExecuteArgs};
use std::path::PathBuf;

use crate::utils::helpers::get_manifest_path;

pub fn build_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<String> {
	let manifest_path = get_manifest_path(path)?;
	// Default values
	let args = ExecuteArgs { manifest_path, ..Default::default() };

	// Execute the build and log the output of the build
	let result = execute(args)?;
	let formatted_result = result.display();

	Ok(formatted_result)
}
