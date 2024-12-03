// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use assert_cmd::{cargo::cargo_bin, Command};
use pop_common::{find_free_port, templates::Template};
use pop_parachains::Parachain;
use std::{fs, path::Path, process::Command as Cmd};
use strum::VariantArray;
use tokio::time::{sleep, Duration};

/// Test the parachain lifecycle: new, build, up, call
#[tokio::test]
async fn parachain_lifecycle() -> Result<()> {
	let temp = tempfile::tempdir().unwrap();
	let temp_dir = temp.path();
	// let temp_dir = Path::new("./"); //For testing locally
	// Test that all templates are generated correctly
	generate_all_the_templates(&temp_dir)?;
	// pop new parachain test_parachain --verify (default)
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
			"--verify",
		])
		.assert()
		.success();
	assert!(temp_dir.join("test_parachain").exists());

	// pop build --release --path "./test_parachain"
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["build", "--release", "--path", "./test_parachain"])
		.assert()
		.success();

	assert!(temp_dir.join("test_parachain/target").exists());

	let temp_parachain_dir = temp_dir.join("test_parachain");
	// pop build spec --output ./target/pop/test-spec.json --id 2222 --type development --relay
	// paseo-local --protocol-id pop-protocol"
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_parachain_dir)
		.args(&[
			"build",
			"spec",
			"--output",
			"./target/pop/test-spec.json",
			"--id",
			"2222",
			"--type",
			"development",
			"--relay",
			"paseo-local",
			"--genesis-state",
			"--genesis-code",
			"--protocol-id",
			"pop-protocol",
		])
		.assert()
		.success();

	// Assert build files have been generated
	assert!(temp_parachain_dir.join("target").exists());
	assert!(temp_parachain_dir.join("target/pop/test-spec.json").exists());
	assert!(temp_parachain_dir.join("target/pop/test-spec-raw.json").exists());
	assert!(temp_parachain_dir.join("target/pop/para-2222.wasm").exists());
	assert!(temp_parachain_dir.join("target/pop/para-2222-genesis-state").exists());

	let content = fs::read_to_string(temp_parachain_dir.join("target/pop/test-spec-raw.json"))
		.expect("Could not read file");
	// Assert custom values has been set propertly
	assert!(content.contains("\"para_id\": 2222"));
	assert!(content.contains("\"tokenDecimals\": 6"));
	assert!(content.contains("\"tokenSymbol\": \"POP\""));
	assert!(content.contains("\"relay_chain\": \"paseo-local\""));
	assert!(content.contains("\"protocolId\": \"pop-protocol\""));

	// Overwrite the config file to manually set the port to test pop call parachain.
	let network_toml_path = temp_parachain_dir.join("network.toml");
	fs::create_dir_all(&temp_parachain_dir)?;
	let random_port = find_free_port();
	let localhost_url = format!("ws://127.0.0.1:{}", random_port);
	fs::write(
		&network_toml_path,
		format!(
			r#"[relaychain]
chain = "paseo-local"

[[relaychain.nodes]]
name = "alice"
rpc_port = {}
validator = true

[[relaychain.nodes]]
name = "bob"
validator = true

[[parachains]]
id = 2000
default_command = "./target/release/parachain-template-node"

[[parachains.collators]]
name = "collator-01"
"#,
			random_port
		),
	)?;

	// pop up parachain -p "./test_parachain"
	let mut cmd = Cmd::new(cargo_bin("pop"))
		.current_dir(&temp_parachain_dir)
		.args(&["up", "parachain", "-f", "./network.toml", "--skip-confirm"])
		.spawn()
		.unwrap();

	// If after 20 secs is still running probably execution is ok, or waiting for user response
	sleep(Duration::from_secs(20)).await;

	assert!(cmd.try_wait().unwrap().is_none(), "the process should still be running");

	// pop call parachain --pallet System --extrinsic remark --args "0x11" --url
	// ws://127.0.0.1:random_port --suri //Alice --skip-confirm
	Command::cargo_bin("pop")
		.unwrap()
		.args(&[
			"call",
			"parachain",
			"--pallet",
			"System",
			"--extrinsic",
			"remark",
			"--args",
			"0x11",
			"--url",
			&localhost_url,
			"--suri",
			"//Alice",
			"--skip-confirm",
		])
		.assert()
		.success();

	// pop call parachain --call 0x00000411 --url ws://127.0.0.1:8833 --suri //Alice --skip-confirm
	Command::cargo_bin("pop")
		.unwrap()
		.args(&[
			"call",
			"parachain",
			"--call",
			"0x00000411",
			"--url",
			"ws://127.0.0.1:8833",
			"--suri",
			"//Alice",
			"--skip-confirm",
		])
		.assert()
		.success();

	// Stop the process
	Cmd::new("kill").args(["-s", "TERM", &cmd.id().to_string()]).spawn()?;

	Ok(())
}

fn generate_all_the_templates(temp_dir: &Path) -> Result<()> {
	for template in Parachain::VARIANTS {
		let parachain_name = format!("test_parachain_{}", template);
		let provider = template.template_type()?.to_lowercase();
		// pop new parachain test_parachain --verify
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
				"--verify",
			])
			.assert()
			.success();
		assert!(temp_dir.join(parachain_name).exists());
	}
	Ok(())
}
