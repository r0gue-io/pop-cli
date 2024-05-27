// SPDX-License-Identifier: GPL-3.0
#[cfg(test)]
use crate::mock::execute;
#[cfg(not(test))]
use contract_build::execute;
use std::path::PathBuf;

use crate::utils::helpers::get_manifest_path;

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
		assert_eq!(result, "\nOriginal wasm size: \u{1b}[1m64.0K\u{1b}[0m, Optimized: \u{1b}[1m32.0K\u{1b}[0m\n\nThe contract was built in \u{1b}[1mDEBUG\u{1b}[0m mode.\n\nYour contract artifacts are ready. You can find them in:\n\u{1b}[1m/path/to/target\u{1b}[0m\n\n  - \u{1b}[1mcontract.contract\u{1b}[0m (code + metadata)\n  - \u{1b}[1mcontract.wasm\u{1b}[0m (the contract's code)\n  - \u{1b}[1mcontract.json\u{1b}[0m (the contract's metadata)");
		Ok(())
	}
}
