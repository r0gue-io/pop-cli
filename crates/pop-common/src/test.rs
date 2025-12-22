// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use duct::cmd;
use std::path::Path;

/// Run tests of a Rust project.
///
/// # Arguments
///
/// * `path` - location of the project.
pub async fn test_project(path: &Path, maybe_test_filter: Option<String>) -> Result<(), Error> {
	// Execute `cargo test` command in the specified directory.
	let mut args = vec!["test".to_string()];
	if let Some(test_filter) = maybe_test_filter {
		args.push(test_filter);
	}
	cmd("cargo", args)
		.dir(path)
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::command_mock::CommandMock;
	use tempfile;

	#[tokio::test]
	async fn test_project_works() -> Result<(), Error> {
		CommandMock::default()
			.execute(async || {
				let temp_dir = tempfile::tempdir()?;
				cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
				test_project(&temp_dir.path().join("test_contract"), None).await?;
				Ok(())
			})
			.await
	}

	#[tokio::test]
	async fn test_project_with_filter_works() -> Result<(), Error> {
		CommandMock::default()
			.execute(async || {
				let temp_dir = tempfile::tempdir()?;
				// Create a lib crate which includes a default test named `it_works`
				cmd("cargo", ["new", "lib_with_tests", "--lib"]).dir(temp_dir.path()).run()?;
				// Run only the `it_works` test using the filter
				test_project(&temp_dir.path().join("lib_with_tests"), Some("it_works".to_string()))
					.await?;
				Ok(())
			})
			.await
	}

	#[tokio::test]
	async fn test_project_wrong_directory() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		assert!(matches!(
			test_project(&temp_dir.path().join(""), None).await,
			Err(Error::TestCommand(..))
		));
		Ok(())
	}
}
