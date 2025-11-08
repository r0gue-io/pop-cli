// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::Result;
use clap::Args;
use duct::cmd;
use std::path::{Path, PathBuf};

/// Launch a frontend dev server.
#[derive(Args, Clone, Default)]
pub(crate) struct FrontendCommand {
	/// Path to the frontend directory
	#[arg(long, short)]
	pub(crate) path: Option<PathBuf>,
}

impl FrontendCommand {
	/// Executes the command.
	pub(crate) fn execute(self, cli: &mut impl cli::traits::Cli) -> Result<()> {
		cli.intro("Launch frontend dev server")?;

		let frontend_dir = if let Some(path) = self.path {
			if path.is_dir() {
				Some(path)
			} else {
				cli.warning(format!("The provided path '{}' is not a directory.", path.display()))?;
				resolve_frontend_dir(&std::env::current_dir()?, cli)?
			}
		} else {
			resolve_frontend_dir(&std::env::current_dir()?, cli)?
		};

		if let Some(frontend_dir) = frontend_dir {
			run_frontend(&frontend_dir)?;
			cli.outro("Frontend dev server launched")?;
		} else {
			cli.outro_cancel("Frontend directory not found")?;
		}

		Ok(())
	}
}

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
	let is_frontend_folder = ["package.json", "bun.lockb", "node_modules"]
		.iter()
		.any(|f| base_dir.join(f).exists());
	if is_frontend_folder {
		cli.info("Detected frontend project in current directory.")?;
		return Ok(Some(base_dir.to_path_buf()));
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
/// * `target` - Path to the frontend project.
pub fn run_frontend(target: &Path) -> Result<()> {
	if is_cmd_available("bun") {
		cmd("bun", &["install"]).dir(target).run()?;
		cmd("bun", &["run", "dev"]).dir(target).run()?;
		return Ok(());
	}
	if is_cmd_available("npm") {
		cmd("npm", &["install"]).dir(target).run()?;
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
	fn resolve_frontend_dir_detects_current_dir_is_frontend_works() -> anyhow::Result<()> {
		let temp = tempdir()?;
		let package_json = temp.path().join("package.json");
		fs::write(&package_json, "{}")?;

		let mut cli = MockCli::new().expect_info("Detected frontend project in current directory.");

		let result = resolve_frontend_dir(temp.path(), &mut cli)?;
		assert_eq!(result.unwrap().canonicalize()?, temp.path().canonicalize()?);

		cli.verify()
	}

	#[test]
	fn is_cmd_available_works() {
		assert!(is_cmd_available("echo"));
		assert!(!is_cmd_available("definitely-not-a-real-command-xyz"));
	}
}
