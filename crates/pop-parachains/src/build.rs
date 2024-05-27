// SPDX-License-Identifier: GPL-3.0
#[cfg(test)]
use crate::mock::cmd;
#[cfg(not(test))]
use duct::cmd;
use std::path::PathBuf;

/// Build the parachain located in the specified `path`.
pub fn build_parachain(path: &Option<PathBuf>) -> anyhow::Result<()> {
	cmd("cargo", vec!["build", "--release"])
		.dir(path.clone().unwrap_or("./".into()))
		.run()?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[test]
	fn test_build_parachain() -> Result<()> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		build_parachain(&Some(PathBuf::from(temp_dir.path())))?;
		Ok(())
	}
}
