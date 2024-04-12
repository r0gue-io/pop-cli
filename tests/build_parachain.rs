#![cfg(feature = "e2e_parachain")]
use anyhow::{Error, Result};
use assert_cmd::Command;
use predicates::prelude::predicate;

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
		.args(&["build", "parachain", "-p", "./test_parachain"])
		.assert()
		.success();

	assert!(temp_dir.path().join("test_parachain/target").exists());
	Ok(())
}

#[test]
fn build_non_parachain_project() {
	let non_parachain = tempfile::tempdir().unwrap();
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&non_parachain)
		.args(&["new", "contract", "non_parachain_rust_project"])
		.assert()
		.success();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&non_parachain)
		.args(&["build", "parachain"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("Build failed: Not a parachain project"));
}
