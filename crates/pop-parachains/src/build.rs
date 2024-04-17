// SPDX-License-Identifier: Apache 2.0
use duct::cmd;
use std::path::PathBuf;

pub fn build_parachain(path: &Option<PathBuf>) -> anyhow::Result<()> {
	cmd("cargo", vec!["build", "--release"])
		.dir(path.clone().unwrap_or("./".into()))
		.run()?;

	Ok(())
}
