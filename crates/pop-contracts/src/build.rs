// SPDX-License-Identifier: GPL-3.0
#[cfg(test)]
use crate::mock::execute;
use crate::utils::helpers::get_manifest_path;
#[cfg(not(test))]
use contract_build::execute;
use std::path::PathBuf;

/// Build the smart contract located in the specified `path`.
pub fn build_smart_contract(path: &Option<PathBuf>) -> anyhow::Result<String> {
	let manifest_path = get_manifest_path(path)?;
	// Default values
	let args = contract_build::ExecuteArgs { manifest_path, ..Default::default() };

	// Execute the build and log the output of the build
	let result = execute(args)?;
	let formatted_result = result.display();

	Ok(formatted_result)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[test]
	fn test_build_contract() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let result = build_smart_contract(&Some(PathBuf::from(temp_dir.path())))?;
		assert!(result.contains("Original wasm size:"));
		assert!(result.contains("64.0K"));
		assert!(result.contains("Optimized:"));
		assert!(result.contains("32.0K"));
		Ok(())
	}
}
