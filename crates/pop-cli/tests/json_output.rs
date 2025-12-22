#![cfg(feature = "integration-tests")]
use anyhow::Result;
use pop_common::pop;
use serde_json::Value;
use tempfile::tempdir;

#[tokio::test]
async fn test_json_output_envelope() -> Result<()> {
	let temp_dir = tempdir()?;
	let working_dir = temp_dir.path();

	// Run a simple command with --json flag
	// pop --json convert address 0x742d35Cc6634C0532925a3b844Bc454e4438f44e
	let mut command = pop(
		working_dir,
		["--json", "convert", "address", "0x742d35Cc6634C0532925a3b844Bc454e4438f44e"],
	);

	let output = command.output().await?;
	assert!(output.status.success());

	let stdout = String::from_utf8(output.stdout)?;
	let response: Value = serde_json::from_str(&stdout)?;

	assert_eq!(response["schema_version"], 1);
	assert_eq!(response["success"], true);
	assert!(response["data"].is_object());
	assert!(response["error"].is_null());

	let data = &response["data"];
	assert_eq!(data["input"], "0x742d35Cc6634C0532925a3b844Bc454e4438f44e");
	assert_eq!(data["output"], "13dKz82CEiU7fKfhfQ5aLpdbXHApLfJH5Z6y2RTZpRwKiNhX");

	Ok(())
}

#[tokio::test]
async fn test_json_output_error() -> Result<()> {
	let temp_dir = tempdir()?;
	let working_dir = temp_dir.path();

	// Run a command that will fail
	// pop --json convert address invalid_address
	let mut command = pop(working_dir, ["--json", "convert", "address", "invalid_address"]);

	let output = command.output().await?;
	// It should exit with 1
	assert!(!output.status.success());

	let stdout = String::from_utf8(output.stdout)?;
	let response: Value = serde_json::from_str(&stdout)?;

	assert_eq!(response["schema_version"], 1);
	assert_eq!(response["success"], false);
	assert!(response["data"].is_null());
	assert!(response["error"].is_object());

	let error = &response["error"];
	assert!(error["code"].is_string());
	assert!(error["message"].is_string());

	Ok(())
}
