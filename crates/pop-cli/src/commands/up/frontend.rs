// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::*},
	install::frontend::has,
};
use anyhow::Result;
use clap::Args;
use duct::cmd;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Launch a frontend dev server.
#[derive(Args, Clone, Default, Serialize)]
pub(crate) struct FrontendCommand {
	/// Path to the frontend directory
	#[serde(skip_serializing)]
	#[arg(long, short)]
	pub(crate) path: Option<PathBuf>,
}

impl FrontendCommand {
	/// Executes the command.
	pub(crate) fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> Result<()> {
		cli.intro("Launch frontend dev server")?;

		let frontend_dir = if let Some(path) = self.path.clone() {
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

		cli.info(self.display())?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut full_message = "pop up frontend".to_string();
		if let Some(path) = &self.path {
			full_message.push_str(&format!(" --path {}", path.display()));
		}
		full_message
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
/// Detection priority: pnpm -> bun -> yarn -> npm
///
/// # Arguments
/// * `target` - Path to the frontend project.
pub fn run_frontend(target: &Path) -> Result<()> {
	let package_manager = detect_package_manager(target);
	match package_manager.as_deref() {
		Some(pm) if has(pm) => {
			cmd(pm, &["install"]).dir(target).run()?;
			cmd(pm, &["run", "dev"]).dir(target).run()?;
			Ok(())
		},
		Some(pm) => Err(anyhow::anyhow!(
			"Detected package manager '{}' from lock files, but it's not installed. Please install it first.",
			pm
		)),
		None => Err(anyhow::anyhow!(
			"No package manager lock file detected. Please ensure the project has been initialized with npm, pnpm, bun, or yarn (e.g., run 'npm install' to create package-lock.json)."
		)),
	}
}

/// Detect which package manager a project uses based on lock files.
///
/// # Arguments
/// * `target` - Path to the frontend project.
fn detect_package_manager(target: &Path) -> Option<String> {
	if target.join("pnpm-lock.yaml").exists() {
		return Some("pnpm".to_string());
	}
	// Bun can use either bun.lockb (binary) or bun.lock (text)
	if target.join("bun.lockb").exists() || target.join("bun.lock").exists() {
		return Some("bun".to_string());
	}
	if target.join("yarn.lock").exists() {
		return Some("yarn".to_string());
	}
	if target.join("package-lock.json").exists() {
		return Some("npm".to_string());
	}

	None
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use std::fs;
	use tempfile::tempdir;

	#[test]
	fn test_frontend_command_display() {
		let cmd = FrontendCommand { path: Some(PathBuf::from("my-frontend")) };
		assert_eq!(cmd.display(), "pop up frontend --path my-frontend");

		let cmd = FrontendCommand { path: None };
		assert_eq!(cmd.display(), "pop up frontend");
	}

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
	fn detect_package_manager_pnpm_works() -> anyhow::Result<()> {
		let temp = tempdir()?;
		fs::write(temp.path().join("pnpm-lock.yaml"), "")?;

		let result = detect_package_manager(temp.path());
		assert_eq!(result, Some("pnpm".to_string()));
		Ok(())
	}

	#[test]
	fn detect_package_manager_bun_works() -> anyhow::Result<()> {
		// Test bun.lockb (binary format)
		let temp = tempdir()?;
		fs::write(temp.path().join("bun.lockb"), "")?;
		let result = detect_package_manager(temp.path());
		assert_eq!(result, Some("bun".to_string()));

		// Test bun.lock (text format)
		let temp = tempdir()?;
		fs::write(temp.path().join("bun.lock"), "")?;
		let result = detect_package_manager(temp.path());
		assert_eq!(result, Some("bun".to_string()));

		Ok(())
	}

	#[test]
	fn detect_package_manager_yarn_works() -> anyhow::Result<()> {
		let temp = tempdir()?;
		fs::write(temp.path().join("yarn.lock"), "")?;

		let result = detect_package_manager(temp.path());
		assert_eq!(result, Some("yarn".to_string()));
		Ok(())
	}

	#[test]
	fn detect_package_manager_npm_works() -> anyhow::Result<()> {
		let temp = tempdir()?;
		fs::write(temp.path().join("package-lock.json"), "")?;

		let result = detect_package_manager(temp.path());
		assert_eq!(result, Some("npm".to_string()));
		Ok(())
	}

	#[test]
	fn detect_package_manager_none_works() -> anyhow::Result<()> {
		let temp = tempdir()?;

		let result = detect_package_manager(temp.path());
		assert_eq!(result, None);
		Ok(())
	}

	#[test]
	fn detect_package_manager_priority_works() -> anyhow::Result<()> {
		let temp = tempdir()?;
		// Create multiple lock files - pnpm should take priority
		fs::write(temp.path().join("pnpm-lock.yaml"), "")?;
		fs::write(temp.path().join("package-lock.json"), "")?;

		let result = detect_package_manager(temp.path());
		assert_eq!(result, Some("pnpm".to_string()));
		Ok(())
	}
}
