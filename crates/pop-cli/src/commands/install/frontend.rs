// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::Confirm},
	common::binary::SemanticVersion,
};
use anyhow::{Result, anyhow};
use duct::cmd;
use std::path::PathBuf;

/// Ensure Bun exists and return the absolute path to the `bun` binary.
///
/// # Arguments
/// * `cli`: Command line interface.
pub fn ensure_bun(cli: &mut impl cli::traits::Cli) -> Result<PathBuf> {
	if let Some(path) = which("bun") {
		return Ok(PathBuf::from(path));
	}
	if !cli
		.confirm("📦 Do you want to proceed with the installation of the following package: bun ?")
		.initial_value(true)
		.interact()?
	{
		return Err(anyhow::anyhow!("🚫 You have cancelled the installation process."));
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

/// Require Node v20+ to be installed.
pub fn ensure_node_v20() -> Result<()> {
	let v = SemanticVersion::try_from("node".to_string()).map_err(|_| {
		anyhow!("NodeJS v20+ required but not found. Install from https://nodejs.org and re-run.")
	})?;
	if v.0 < 20 {
		return Err(anyhow!(format!(
			"NodeJS v20+ required. Detected {}.{}.{}. Please upgrade Node and re-run.",
			v.0, v.1, v.2
		)));
	}
	Ok(())
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
