// SPDX-License-Identifier: GPL-3.0

#![cfg(all(feature = "contract", not(feature = "chain")))]

use std::process::Command;

#[test]
fn json_test_outputs_envelope_in_contract_only_mode() -> anyhow::Result<()> {
	let temp_dir = tempfile::tempdir()?;
	let project_name = "json_contract_only_project";
	let project_path = temp_dir.path().join(project_name);

	let create_project = Command::new("cargo")
		.args(["new", project_name, "--bin"])
		.current_dir(temp_dir.path())
		.output()?;
	assert!(
		create_project.status.success(),
		"failed to create test project: {}",
		String::from_utf8_lossy(&create_project.stderr)
	);

	let output = Command::new(env!("CARGO_BIN_EXE_pop"))
		.args(["--json", "test", "--path", project_path.to_str().unwrap()])
		.output()?;
	assert!(
		output.status.success(),
		"expected success, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stdout = String::from_utf8(output.stdout)?;
	let line = stdout
		.lines()
		.find(|line| !line.trim().is_empty())
		.ok_or_else(|| anyhow::anyhow!("missing JSON envelope on stdout"))?;
	let json: serde_json::Value = serde_json::from_str(line)?;

	assert_eq!(json["schema_version"], 1);
	assert_eq!(json["success"], true);
	assert_eq!(json["data"]["command"], "cargo test");
	assert_eq!(json["data"]["success"], true);
	assert!(json.get("error").is_none());
	Ok(())
}
