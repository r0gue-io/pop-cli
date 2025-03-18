// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use duct::cmd;
use std::path::Path;

/// Run tests of a Rust project.
///
/// # Arguments
///
/// * `path` - location of the project.
pub fn test_project(path: Option<&Path>) -> Result<(), Error> {
	// Execute `cargo test` command in the specified directory.
	cmd("cargo", vec!["test"])
		.dir(path.unwrap_or_else(|| Path::new("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile;

	#[test]
	fn test_project_works() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
		test_project(Some(&temp_dir.path().join("test_contract")))?;
		Ok(())
	}

	#[test]
	fn test_project_wrong_directory() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		assert!(matches!(
			test_project(Some(&temp_dir.path().join(""))),
			Err(Error::TestCommand(..))
		));
		Ok(())
	}
}
