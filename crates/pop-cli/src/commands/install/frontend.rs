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

	ensure_node_v20(skip_confirm, cli).await?;
	ensure_bun(skip_confirm, cli).await?;
	Ok(())
}

/// Require Node v20+ is installed, and install if not present.
///
/// # Arguments
/// * `skip_confirm`: If true, skip user confirmation prompts.
/// * `cli`: Command line interface (for interactive confirm).
pub async fn ensure_node_v20(skip_confirm: bool, cli: &mut impl cli::traits::Cli) -> Result<()> {
	if has("node") {
		let v = SemanticVersion::try_from("node".to_string()).map_err(|_| {
			anyhow!("NodeJS detected but version check failed. Make sure `node --version` works.")
		})?;

		if v.0 >= MIN_NODE_VERSION {
			return Ok(());
		}
	}
	if !skip_confirm &&
		!cli.confirm("📦 NodeJS v20+ is required. Install/upgrade now via nvm?")
			.initial_value(true)
			.interact()?
	{
		return Err(anyhow!(
			"🚫 You have cancelled the installation process. NodeJS v20+ is required. Install from https://nodejs.org and re-run."
		));
	}
	let nvm_script = install_nvm(cli).await?;

	// Install node via nvm (need to source nvm first)
	let install_cmd = format!(r#"source "{}" && nvm install node --lts"#, nvm_script);
	cmd("bash", vec!["-c", &install_cmd]).run()?;
	Ok(())
}

/// Ensure Bun exists and return the absolute path to the `bun` binary.
///
/// # Arguments
/// * `skip_confirm`: If true, skip user confirmation prompts.
/// * `cli`: Command line interface.
pub async fn ensure_bun(skip_confirm: bool, cli: &mut impl cli::traits::Cli) -> Result<PathBuf> {
	if let Some(path) = which("bun") {
		return Ok(PathBuf::from(path));
	}
	if !skip_confirm &&
		!cli.confirm(
			"📦 Do you want to proceed with the installation of the following package: bun?",
		)
		.initial_value(true)
		.interact()?
	{
		return Err(anyhow!(
			"🚫 You have cancelled the installation process. Bun is required. Install from https://bun.com/ and re-run."
		));
	}
	// Install Bun using the official installer script
	run_external_script("https://bun.sh/install", &[]).await?;

	// Try to locate bun in PATH again
	if let Some(path) = which("bun") {
		return Ok(PathBuf::from(path));
	}
	// Use the default install location from the official installer if not found in PATH
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

/// Check if a command is available on the system.
///
/// # Arguments
/// * `bin` - The binary name to check.
pub fn has(bin: &str) -> bool {
	which(bin).is_some()
}

fn which(bin: &str) -> Option<String> {
	cmd("which", vec![bin]).read().ok()
}

/// Install nvm (Node Version Manager) if not already present.
/// Returns the path to the nvm.sh script.
async fn install_nvm(cli: &mut impl cli::traits::Cli) -> Result<String> {
	let home = std::env::var("HOME").map_err(|_| anyhow!("HOME not set"))?;
	let nvm_script = format!("{home}/.nvm/nvm.sh");

	let nvm_is_installed = std::path::Path::new(&nvm_script).exists();
	if nvm_is_installed {
		cli.info("ℹ️ nvm already installed.".to_string())?;
	} else {
		run_external_script(NVM_INSTALL_SCRIPT, &[]).await?;
	}
	Ok(nvm_script)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn has_works() {
		assert!(has("sh"));
		assert!(!has("this_binary_should_not_exist_12345"));
	}

	#[test]
	fn which_works() {
		let result = which("sh");
		assert!(result.is_some());
		let result = which("this_binary_should_not_exist_12345");
		assert!(result.is_none());
	}
}
