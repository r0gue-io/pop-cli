// SPDX-License-Identifier: GPL-3.0

use anyhow::{Error, Result};
use assert_cmd::Command;
use pop_contracts::Template;
use predicates::prelude::*;
use strum::VariantArray;
use tempfile::TempDir;

/// Test the contract lifecycle: new, build, test, up, call
#[test]
fn contract_lifecycle() -> Result<()> {
	let temp_dir = tempfile::tempdir().unwrap();
	// Test that all templates are generated correctly
	generate_all_the_templates(&temp_dir)?;
	// pop new contract test_contract (default)
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["new", "contract", "test_contract"])
		.assert()
		.success();
	assert!(temp_dir.path().join("test_contract").exists());

	// pop build contract
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir.path())
		.args(&["build", "contract", "--path", "./test_contract", "--release"])
		.assert()
		.success();
	// Verify that the folder target has been created
	assert!(temp_dir.path().join("test_contract/target").exists());
	// Verify that all the artifacts has been generated
	assert!(temp_dir.path().join("test_contract/target/ink/test_contract.contract").exists());
	assert!(temp_dir.path().join("test_contract/target/ink/test_contract.wasm").exists());
	assert!(temp_dir.path().join("test_contract/target/ink/test_contract.json").exists());

	// pop test contract
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir.path().join("test_contract"))
		.args(&["test", "contract"])
		.assert()
		.success();

	// pop test contract --features e2e-tests --node path
	// Command::cargo_bin("pop")
	// 	.unwrap()
	// 	.current_dir(&temp_dir.path().join("test_contract"))
	// 	.args(&["test", "contract", "--features", "e2e-tests", "--node", "path" ])
	// 	.assert()
	// 	.success();
	Ok(())
}

fn generate_all_the_templates(temp_dir: &TempDir) -> Result<()> {
	for template in Template::VARIANTS {
		let contract_name = format!("test_contract_{}", template);
		let contract_type = template.contract_type()?.to_lowercase();
		// pop new parachain test_parachain
		Command::cargo_bin("pop")
			.unwrap()
			.current_dir(&temp_dir)
			.args(&[
				"new",
				"contract",
				&contract_name,
				"--contract-type",
				&contract_type,
				"--template",
				&template.to_string(),
			])
			.assert()
			.success();
		assert!(temp_dir.path().join(contract_name).exists());
	}
	Ok(())
}

#[test]
fn test_contract_build_fails_if_no_contract_exists() -> Result<(), Error> {
	// pop build contract
	Command::cargo_bin("pop")
		.unwrap()
		.args(&["build", "contract"])
		.assert()
		.failure()
		.stderr(predicate::str::contains("Error: No 'ink' dependency found"));

	Ok(())
}
