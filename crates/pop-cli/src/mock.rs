// SPDX-License-Identifier: GPL-3.0
use std::path::PathBuf;

// Mock the build function for parachains
#[cfg(test)]
pub fn build_parachain(_path: &Option<PathBuf>) -> anyhow::Result<()> {
	Ok(())
}

// Mock the build function for smart contracts
#[cfg(test)]
pub fn build_smart_contract(_path: &Option<PathBuf>) -> anyhow::Result<String> {
	Ok("Ok".to_string())
}
