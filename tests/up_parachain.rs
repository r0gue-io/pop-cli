use anyhow::{anyhow, Result};
use assert_cmd::{cargo::cargo_bin, Command as AssertCmd};
use std::{
	fs,
	path::PathBuf,
	process::{Command, Stdio},
};
use tokio::time::{sleep, Duration};

fn setup_test_environment() -> Result<()> {
	// pop new parachain test_parachain
	AssertCmd::cargo_bin("pop")
		.unwrap()
		.args(&["new", "parachain", "test_parachain"])
		.assert()
		.success();
	println!("Parachain created, building it");

	// pop build parachain test_parachain
	AssertCmd::cargo_bin("pop")
		.unwrap()
		.args(&["build", "parachain", "-p", "./test_parachain"])
		.assert()
		.success();

	println!("Parachain built");

	Ok(())
}

fn clean_test_environment() -> Result<()> {
	if let Err(err) = fs::remove_dir_all("test_parachain") {
		eprintln!("Failed to delete directory: {}", err);
	}
	Ok(())
}

#[tokio::test]
async fn test_parachain_up() -> Result<()> {
	setup_test_environment()?;

	println!("pop up parachain -f ./test_parachain/network.toml");
	let mut dir = PathBuf::new();
	dir.push("test_parachain");

	// pop up parachain
	let mut cmd = Command::new(cargo_bin("pop"))
		.current_dir(dir)
		.stdout(Stdio::piped())
		.args(&["up", "parachain", "-f", "./network.toml"])
		.spawn()
		.unwrap();

	// If after 15 secs is still running probably execution is ok
	sleep(Duration::from_secs(15)).await;
	assert!(cmd.try_wait().unwrap().is_none(), "the process should still be running");

	// Stop the process
	Command::new("kill").args(["-s", "TERM", &cmd.id().to_string()]).spawn()?;

	// Clean up
	clean_test_environment()?;

	Ok(())
}
