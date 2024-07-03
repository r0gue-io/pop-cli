// SPDX-License-Identifier: GPL-3.0

use anyhow::{Error, Result};
use assert_cmd::Command;

fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
	let temp_dir = tempfile::tempdir().unwrap();
	// pop new parachain test_parachain
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["new", "parachain", "test_parachain"])
		.assert()
		.success();

	Ok(temp_dir)
}

#[test]
fn test_parachain_build_after_instantiating_template() -> Result<()> {
	let temp_dir = setup_test_environment()?;

	// pop build contract -p "./test_parachain"
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["build", "parachain", "--path", "./test_parachain"])
		.assert()
		.success();

	assert!(temp_dir.path().join("test_parachain/target").exists());
	Ok(())
}
