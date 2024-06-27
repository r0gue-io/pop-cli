// SPDX-License-Identifier: GPL-3.0

use std::{fs, path::Path};

use anyhow::Result;
use assert_cmd::Command;
use pop_parachains::Template;
use strum::VariantArray;

/// Test the parachain lifecycle: new, build, up
#[test]
fn parachain_lifecycle() -> Result<()> {
	let temp = tempfile::tempdir().unwrap();
	let temp_dir = temp.path();
	//let temp_dir = Path::new("./");
	// Test that all templates are generated correctly
	generate_all_the_templates(&temp_dir)?;
	// pop new parachain test_parachain (default)
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&[
			"new",
			"parachain",
			"test_parachain",
			"--symbol",
			"POP",
			"--decimals",
			"6",
			"--endowment",
			"1u64 << 60",
		])
		.assert()
		.success();
	assert!(temp_dir.join("test_parachain").exists());

	// pop build contract -p "./test_parachain"
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["build", "parachain", "-p", "./test_parachain", "--para_id", "2000"])
		.assert()
		.success();

	assert!(temp_dir.join("test_parachain/target").exists());
	// Assert build files has been generated
	assert!(temp_dir.join("test_parachain/raw-parachain-chainspec.json").exists());
	assert!(temp_dir.join("test_parachain/para-2000-wasm").exists());
	assert!(temp_dir.join("test_parachain/para-2000-genesis-state").exists());

	let content = fs::read_to_string(temp_dir.join("test_parachain/raw-parachain-chainspec.json"))
		.expect("Could not read file");
	// Assert the custom values has been set propertly
	assert!(content.contains("\"para_id\": 2000"));
	assert!(content.contains("\"tokenDecimals\": 6"));
	assert!(content.contains("\"tokenSymbol\": \"POP\""));

	Ok(())
}

fn generate_all_the_templates(temp_dir: &Path) -> Result<()> {
	for template in Template::VARIANTS {
		let parachain_name = format!("test_parachain_{}", template);
		let provider = template.provider()?.to_lowercase();
		// pop new parachain test_parachain
		Command::cargo_bin("pop")
			.unwrap()
			.current_dir(&temp_dir)
			.args(&[
				"new",
				"parachain",
				&parachain_name,
				&provider,
				"--template",
				&template.to_string(),
			])
			.assert()
			.success();
		assert!(temp_dir.join(parachain_name).exists());
	}
	Ok(())
}
