// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "parachain")]

use anyhow::Result;
use assert_cmd::cargo::cargo_bin;
use pop_common::{find_free_port, templates::Template};
use pop_parachains::Parachain;
use std::{ffi::OsStr, fs, path::Path, process::Command, thread::sleep, time::Duration};
use strum::VariantArray;

// Test that all templates are generated correctly
#[test]
fn generate_all_the_templates() -> Result<()> {
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();

	for template in Parachain::VARIANTS {
		let parachain_name = format!("test_parachain_{}", template);
		let provider = template.template_type()?.to_lowercase();
		// pop new parachain test_parachain --verify
		let mut command = pop(
			&temp_dir,
			&[
				"new",
				"parachain",
				&parachain_name,
				&provider,
				"--template",
				&template.to_string(),
				"--verify",
			],
		);
		assert!(command.spawn()?.wait()?.success());
		assert!(temp_dir.join(parachain_name).exists());
	}
	Ok(())
}

/// Test the parachain lifecycle: new, build, up, call.
#[test]
fn parachain_lifecycle() -> Result<()> {
	// Always use the same directory to ensure effective caching
	let temp_dir = Path::new("./");

	// pop new parachain test_parachain --verify (default)
	let working_dir = temp_dir.join("test_parachain");
	if !working_dir.exists() {
		let mut command = pop(
			&temp_dir,
			&[
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
			],
		);
		assert!(command.spawn()?.wait()?.success());
		assert!(working_dir.exists());
	}

	// pop build --release
	let mut command = pop(&working_dir, &["build", "--release"]);
	assert!(command.spawn()?.wait()?.success());
	assert!(temp_dir.join("test_parachain/target/release").exists());

	// pop build spec --output ./target/pop/test-spec.json --id 2222 --type development --relay
	// paseo-local --protocol-id pop-protocol" --chain local --skip-deterministic-build
	let mut command = pop(
		&working_dir,
		&[
			"build",
			"spec",
			"--output",
			"./target/pop/test-spec.json",
			"--id",
			"2222",
			"--type",
			"development",
			"--chain",
			"local",
			"--relay",
			"paseo-local",
			"--profile",
			"release",
			"--genesis-state",
			"--genesis-code",
			"--protocol-id",
			"pop-protocol",
			"--skip-deterministic-build",
		],
	);
	assert!(command.spawn()?.wait()?.success());

	// Assert build files have been generated
	assert!(working_dir.join("target").exists());
	assert!(working_dir.join("target/pop/test-spec.json").exists());
	assert!(working_dir.join("target/pop/test-spec-raw.json").exists());
	assert!(working_dir.join("target/pop/para-2222.wasm").exists());
	assert!(working_dir.join("target/pop/para-2222-genesis-state").exists());

	let content = fs::read_to_string(working_dir.join("target/pop/test-spec-raw.json"))
		.expect("Could not read file");
	// Assert custom values have been set properly
	assert!(content.contains("\"para_id\": 2222"));
	assert!(content.contains("\"tokenDecimals\": 6"));
	assert!(content.contains("\"tokenSymbol\": \"POP\""));
	assert!(content.contains("\"relay_chain\": \"paseo-local\""));
	assert!(content.contains("\"protocolId\": \"pop-protocol\""));
	assert!(content.contains("\"id\": \"local_testnet\""));

	// Overwrite the config file to manually set the port to test pop call parachain.
	let network_toml_path = working_dir.join("network.toml");
	fs::create_dir_all(&working_dir)?;
	let random_port = find_free_port(None);
	let localhost_url = format!("ws://127.0.0.1:{}", random_port);
	fs::write(
		&network_toml_path,
		format!(
			r#"[relaychain]
chain = "paseo-local"

[[relaychain.nodes]]
name = "alice"
validator = true

[[relaychain.nodes]]
name = "bob"
validator = true

[[parachains]]
id = 2000
default_command = "./target/release/parachain-template-node"

[[parachains.collators]]
name = "collator-01"
rpc_port = {random_port}
"#
		),
	)?;

	// `pop up network -f ./network.toml --skip-confirm`
	let mut command = pop(
		&working_dir,
		&["up", "network", "./network.toml", "-r", "stable2412", "--verbose", "--skip-confirm"],
	);
	let mut up = command.spawn()?;

	// Wait for the networks to initialize. Increased timeout to accommodate CI environment delays.
	let wait = Duration::from_secs(50);
	println!("waiting for {wait:?} for network to initialize...");
	sleep(wait);

	// `pop call chain --pallet System --function remark --args "0x11" --url
	// ws://127.0.0.1:random_port --suri //Alice --skip-confirm`
	let mut command = pop(
		&working_dir,
		&[
			"call",
			"chain",
			"--pallet",
			"System",
			"--function",
			"remark",
			"--args",
			"0x11",
			"--url",
			&localhost_url,
			"--suri",
			"//Alice",
			"--skip-confirm",
		],
	);
	assert!(command.spawn()?.wait()?.success());

	// pop call chain --call 0x00000411 --url ws://127.0.0.1:random_port --suri //Alice
	// --skip-confirm
	let mut command = pop(
		&working_dir,
		&[
			"call",
			"chain",
			"--call",
			"0x00000411",
			"--url",
			&localhost_url,
			"--suri",
			"//Alice",
			"--skip-confirm",
		],
	);
	assert!(command.spawn()?.wait()?.success());

	assert!(up.try_wait()?.is_none(), "the process should still be running");
	// Stop the process
	Command::new("kill").args(["-s", "SIGINT", &up.id().to_string()]).spawn()?;

	Ok(())
}

fn pop(dir: &Path, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Command {
	let mut command = Command::new(cargo_bin("pop"));
	command.current_dir(dir).args(args);
	println!("{command:?}");
	command
}
