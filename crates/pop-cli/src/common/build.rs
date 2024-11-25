// SPDX-License-Identifier: GPL-3.0

use pop_common::manifest::from_path;
use std::path::Path;
/// Checks if a contract has been built by verifying the existence of the build directory and the
/// <name>.contract file.
///
/// # Arguments
/// * `path` - An optional path to the project directory. If no path is provided, the current
///   directory is used.
pub fn has_contract_been_built(path: Option<&Path>) -> bool {
	let project_path = path.unwrap_or_else(|| Path::new("./"));
	let manifest = match from_path(Some(project_path)) {
		Ok(manifest) => manifest,
		Err(_) => return false,
	};
	if manifest.package.as_ref().is_none() {
		false
	} else {
		let contract_name = manifest.package().name();
		project_path.join("target/ink").exists() &&
			project_path.join(format!("target/ink/{}.contract", contract_name)).exists()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use duct::cmd;
	use std::fs::{self, File};

	#[test]
	fn has_contract_been_built_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(&path).run()?;
		let contract_path = path.join(name);
		assert!(!has_contract_been_built(Some(&contract_path)));

		cmd("cargo", ["build"]).dir(&contract_path).run()?;
		// Mock build directory
		fs::create_dir(&contract_path.join("target/ink"))?;
		assert!(!has_contract_been_built(Some(&path.join(name))));
		// Create a mocked .contract file inside the target directory
		File::create(contract_path.join(format!("target/ink/{}.contract", name)))?;
		assert!(has_contract_been_built(Some(&path.join(name))));
		Ok(())
	}
}
