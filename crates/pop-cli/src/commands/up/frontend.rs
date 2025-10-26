// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::Result;
use duct::cmd;
use std::path::{Path, PathBuf};

/// Resolve a frontend directory. Default is ./frontend under current working directory. If not
/// present, prompt the user.
///
/// # Arguments
/// * `base_dir`: Base directory to resolve the frontend directory from.
/// * `cli`: Command line interface
pub fn resolve_frontend_dir(
	base_dir: &Path,
	cli: &mut impl cli::traits::Cli,
) -> Result<Option<PathBuf>> {
	let frontend_folder = base_dir.join("frontend");
	if frontend_folder.is_dir() {
		return Ok(Some(frontend_folder));
	}
	let pick = cli
		.input(
			"Frontend directory not found at ./frontend. Provide a path to the frontend directory:",
		)
		.placeholder("./frontend")
		.interact()?;
	let path = PathBuf::from(pick);
	if !path.is_dir() {
		cli.warning(format!(
			"The provided path '{}' is not a directory. Skipping frontend.",
			path.display()
		))?;
		return Ok(None);
	}
	Ok(Some(path))
}

/// Decide which command to use to run locally the frontend and run it.
///
/// # Arguments
/// * `target` - Location where the smart contract will be created.
pub fn run_frontend(target: &Path) -> Result<()> {
	if is_cmd_available("bun") {
		cmd("bun", &["run", "dev"]).dir(target).run()?;
		return Ok(());
	}
	if is_cmd_available("npm") {
		cmd("npm", &["run", "dev"]).dir(target).run()?;
		return Ok(());
	}
	Err(anyhow::anyhow!("No supported package manager found. Please install bun or npm."))
}

fn is_cmd_available(bin: &str) -> bool {
	std::process::Command::new(bin)
		.arg("--version")
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.status()
		.map(|s| s.success())
		.unwrap_or(false)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use std::fs;
	use tempfile::tempdir;

	#[test]
	fn resolve_frontend_dir_finds_existing_frontend_works() -> anyhow::Result<()> {
		let temp = tempdir()?;
		let frontend_path = temp.path().join("frontend");
		fs::create_dir(&frontend_path)?;

		let mut cli = MockCli::new();
		let result = resolve_frontend_dir(temp.path(), &mut cli)?;

		assert_eq!(result.unwrap().canonicalize()?, frontend_path.canonicalize()?);
		cli.verify()
	}

	#[test]
	fn resolve_frontend_dir_prompts_for_path() -> anyhow::Result<()> {
		let temp = tempdir()?;
		let custom_dir = temp.path().join("custom_frontend");
		fs::create_dir(&custom_dir)?;

		let mut cli = MockCli::new().expect_input(
			"Frontend directory not found at ./frontend. Provide a path to the frontend directory:",
			custom_dir.to_string_lossy().to_string(),
		);

		let result = resolve_frontend_dir(temp.path(), &mut cli)?;
		assert_eq!(result, Some(custom_dir));
		cli.verify()
	}

	#[test]
	fn is_cmd_available_works() {
		assert!(is_cmd_available("echo"));
		assert!(!is_cmd_available("definitely-not-a-real-command-xyz"));
	}
}
