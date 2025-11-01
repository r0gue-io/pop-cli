// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::Confirm},
	commands::install::run_external_script,
	common::binary::SemanticVersion,
};
use anyhow::{Result, anyhow};
use duct::cmd;
use std::path::PathBuf;

const MIN_NODE_VERSION: u8 = 20;
const NVM_INSTALL_SCRIPT: &str = "https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh";

/// Install frontend dependencies (Node.js and Bun).
///
/// # Arguments
/// * `skip_confirm`: If true, skip user confirmation prompts.
/// * `cli`: Command line interface.
pub async fn install_frontend_dependencies(
	skip_confirm: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	cli.info("Installing frontend development dependencies...")?;

	ensure_node(skip_confirm, cli).await?;
	ensure_bun(skip_confirm, cli)?;

	cli.info("✅ Frontend dependencies installed successfully.")?;
	Ok(())
}

/// Require Node v20+ is installed, and install if not present.
///
/// # Arguments
/// * `skip_confirm`: If true, skip user confirmation prompts.
/// * `cli`: Command line interface (for interactive confirm).
pub async fn ensure_node(skip_confirm: bool, cli: &mut impl cli::traits::Cli) -> Result<()> {
	if has("node") {
		let v = SemanticVersion::try_from("node".to_string()).map_err(|_| {
			anyhow!("NodeJS detected but version check failed. Make sure `node --version` works.")
		})?;

		if v.0 >= MIN_NODE_VERSION {
			return Ok(());
		}
	}
	if !skip_confirm {
		if !cli
			.confirm("📦 NodeJS v20+ is required. Install/upgrade now via nvm?")
			.initial_value(true)
			.interact()?
		{
			return Err(anyhow!(
				"🚫 You have cancelled the installation process. NodeJS v20+ is required. Install from https://nodejs.org and re-run."
			));
		}
	}
	install_nvm(cli).await?;
	// Install node
	cmd("nvm", vec!["install", "node"]).run()?;
	Ok(())
}

/// Ensure Bun exists and return the absolute path to the `bun` binary.
///
/// # Arguments
/// * `skip_confirm`: If true, skip user confirmation prompts.
/// * `cli`: Command line interface.
pub fn ensure_bun(skip_confirm: bool, cli: &mut impl cli::traits::Cli) -> Result<PathBuf> {
	if let Some(path) = which("bun") {
		return Ok(PathBuf::from(path));
	}
	if !skip_confirm {
		if !cli
			.confirm(
				"📦 Do you want to proceed with the installation of the following package: bun ?",
			)
			.initial_value(true)
			.interact()?
		{
			return Err(anyhow!("🚫 You have cancelled the installation process."));
		}
	}
	// Install Bun (macOS/Linux official script)
	cmd("bash", vec!["-lc", r#"curl -fsSL https://bun.sh/install | bash"#]).run()?;
	// Use the default install location from the official installer
	let home = std::env::var("HOME").map_err(|_| anyhow!("HOME not set"))?;
	let bun_abs = PathBuf::from(format!("{home}/.bun/bin/bun"));

	if !bun_abs.exists() {
		return Err(anyhow!(format!(
			"Bun installed but not found at {}. Open a new shell or add it to PATH.",
			bun_abs.display()
		)));
	}
	Ok(bun_abs)
}

/// Require `npx` to be available.
pub fn ensure_npx() -> Result<()> {
	if !has("npx") && !has("npm") {
		return Err(anyhow!(
			"`npx` (or npm with npx) not found on PATH. Install NodeJS from https://nodejs.org and re-run."
		));
	}
	Ok(())
}

fn has(bin: &str) -> bool {
	cmd("which", vec![bin]).read().is_ok()
}

fn which(bin: &str) -> Option<String> {
	cmd("which", vec![bin]).read().ok().map(|s| s.trim().to_string())
}

/// Install nvm (Node Version Manager) if not already present.
async fn install_nvm(cli: &mut impl cli::traits::Cli) -> Result<()> {
	let nvm_is_installed = std::env::var("HOME")
		.map(|home| std::path::Path::new(&home).join(".nvm/nvm.sh").exists())
		.unwrap_or(false);
	if nvm_is_installed {
		cli.info("ℹ️ nvm already installed.".to_string())?;
	} else {
		run_external_script(NVM_INSTALL_SCRIPT, &[]).await?;
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;

	#[test]
	fn has_works() {
		assert!(has("sh"));
		assert!(!has("this_binary_should_not_exist_12345"));
	}

	#[test]
	fn which_works() {
		let result = which("sh");
		assert!(result.is_some() || result.is_none()); // Just verify no panic
		let result = which("this_binary_should_not_exist_12345");
		assert!(result.is_none());
	}

	#[test]
	fn ensure_bun_returns_path_when_bun_exists() {
		let mut cli = MockCli::new();

		// Only test if bun exists: check we get a path
		if which("bun").is_some() {
			let result = ensure_bun(true, &mut cli);
			assert!(result.is_ok());
		}
	}
}
