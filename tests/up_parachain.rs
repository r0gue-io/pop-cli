use anyhow::Result;
use assert_cmd::{cargo::cargo_bin, Command as AssertCmd};
use std::{
	fs,
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

	// pup build parachain test_parachain
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
	let _ = setup_test_environment();

	println!("pop parachain up -f ./tests/integration_tests.toml");

	// pop up parachain
	let mut cmd = Command::new(cargo_bin("pop"))
		.stdout(Stdio::piped())
		.args(&["up", "parachain", "-f", "./tests/integration_tests.toml"])
		.spawn()
		.unwrap();

	// Ideally should parse the output of the command

	//let stdout = cmd.stdout.take().unwrap();

	// thread::spawn(move || {
	//     let reader = BufReader::new(stdout);
	//     for line in reader.lines() {
	//         let output = line.unwrap();
	//         println!("All the lines: {:?}", output);
	//     }
	// });

	// If after 15 secs is still running probably excution is ok
	sleep(Duration::from_secs(15)).await;
	assert!(cmd.try_wait().unwrap().is_none(), "the process should still be running");

	// Stop the process
	Command::new("kill").args(["-s", "TERM", &cmd.id().to_string()]).spawn()?;

	// Clean up
	let _ = clean_test_environment();

	Ok(())
}
