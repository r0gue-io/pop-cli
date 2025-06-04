// SPDX-License-Identifier: GPL-3.0

#![cfg(feature = "parachain")]

use anyhow::Result;
use assert_cmd::cargo::cargo_bin;
use pop_common::{find_free_port, templates::Template};
use pop_parachains::Parachain;
use similar::{ChangeTag, TextDiff};
use std::{fs, path::Path, process::Command, ffi::OsStr, thread::sleep, time::Duration};
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

<<<<<<< HEAD
/// Test the parachain lifecycle: new, add pallet ,build, up, call.
#[tokio::test]
async fn parachain_lifecycle() -> Result<()> {
	// For testing locally: set to `true`
	const LOCAL_TESTING: bool = false;

	let temp = tempfile::tempdir()?;
	let temp_dir = match LOCAL_TESTING {
		true => Path::new("./"),
		false => temp.path(),
	};

	// pop new parachain test_parachain --verify (default)
	let working_dir = temp_dir.join("test_parachain");
	if !working_dir.exists() {
		let mut command = pop(
			&temp_dir,
			&[
			"new",
			"parachain",
			"test_parachain",
			"--release-tag",
			"polkadot-stable2412",
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

	// pop add correctly adds pallet-contracts to the template
	let runtime_path = working_dir.join("runtime");

	let workspace_manifest_path = working_dir.join("Cargo.toml");
	let runtime_manifest_path = runtime_path.join("Cargo.toml");
	let runtime_lib_path = runtime_path.join("src").join("lib.rs");
	let pallet_configs_path = runtime_path.join("src").join("configs");
	let pallet_configs_mod_path = pallet_configs_path.join("mod.rs");
	let contracts_pallet_config_path = pallet_configs_path.join("contracts.rs");

	assert!(!contracts_pallet_config_path.exists());

	let runtime_lib_content_before = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let pallet_configs_mod_content_before =
		std::fs::read_to_string(&pallet_configs_mod_path).unwrap();
	let workspace_manifest_content_before =
		std::fs::read_to_string(&workspace_manifest_path).unwrap();
	let runtime_manifest_content_before = std::fs::read_to_string(&runtime_manifest_path).unwrap();

	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&test_parachain)
		.args(&["add", "pallet", "-p", "contracts", "-v", "39.0.0"])
		.assert()
		.success();

	let runtime_lib_content_after = std::fs::read_to_string(&runtime_lib_path).unwrap();
	let pallet_configs_mod_content_after =
		std::fs::read_to_string(&pallet_configs_mod_path).unwrap();
	let workspace_manifest_content_after =
		std::fs::read_to_string(&workspace_manifest_path).unwrap();
	let runtime_manifest_content_after = std::fs::read_to_string(&runtime_manifest_path).unwrap();
	let contracts_pallet_config_content =
		std::fs::read_to_string(&contracts_pallet_config_path).unwrap();

	let runtime_lib_diff =
		TextDiff::from_lines(&runtime_lib_content_before, &runtime_lib_content_after);
	let pallet_configs_mod_diff =
		TextDiff::from_lines(&pallet_configs_mod_content_before, &pallet_configs_mod_content_after);
	let workspace_manifest_diff =
		TextDiff::from_lines(&workspace_manifest_content_before, &workspace_manifest_content_after);
	let runtime_manifest_diff =
		TextDiff::from_lines(&runtime_manifest_content_before, &runtime_manifest_content_after);

	let expected_inserted_lines_runtime_lib = vec![
		"\n",
		"    #[runtime::pallet_index(34)]\n",
		"    pub type Contracts = pallet_contracts;\n",
	];
	let expected_inserted_lines_configs_mod = vec!["mod contracts;\n"];
	let expected_inserted_lines_workspace_manifest =
		vec!["pallet-contracts = { version = \"39.0.0\", default-features = false }\n"];

	let expected_inserted_lines_runtime_manifest = vec![
		"pallet-contracts = { workspace = true, default-features = false }\n",
		"  \"xcm/std\", \"pallet-contracts/std\",\n",
		"  \"xcm-executor/runtime-benchmarks\", \"pallet-contracts/runtime-benchmarks\",\n",
		"  \"sp-runtime/try-runtime\", \"pallet-contracts/try-runtime\",\n",
	];

	let mut inserted_lines_runtime_lib = Vec::with_capacity(3);
	let mut inserted_lines_configs_mod = Vec::with_capacity(1);
	let mut inserted_lines_workspace_manifest = Vec::with_capacity(1);
	let mut inserted_lines_runtime_manifest = Vec::with_capacity(1);

	for change in runtime_lib_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("no deletion expected"),
			ChangeTag::Insert => inserted_lines_runtime_lib.push(change.value()),
			_ => (),
		}
	}

	for change in pallet_configs_mod_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("no deletion expected"),
			ChangeTag::Insert => inserted_lines_configs_mod.push(change.value()),
			_ => (),
		}
	}

	for change in workspace_manifest_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Delete => panic!("no deletion expected"),
			ChangeTag::Insert => inserted_lines_workspace_manifest.push(change.value()),
			_ => (),
		}
	}

	for change in runtime_manifest_diff.iter_all_changes() {
		match change.tag() {
			ChangeTag::Insert => inserted_lines_runtime_manifest.push(change.value()),
			_ => (),
		}
	}

	assert_eq!(expected_inserted_lines_runtime_lib, inserted_lines_runtime_lib);
	assert_eq!(expected_inserted_lines_configs_mod, inserted_lines_configs_mod);
	assert_eq!(expected_inserted_lines_workspace_manifest, inserted_lines_workspace_manifest);
	assert_eq!(expected_inserted_lines_runtime_manifest, inserted_lines_runtime_manifest);

	assert_eq!(
		contracts_pallet_config_content,
		r#"use crate::{Balances, Runtime, RuntimeCall, RuntimeEvent, RuntimeHoldReason};
use frame_support::{derive_impl, parameter_types};

parameter_types! {
    pub Schedule : pallet_contracts::Schedule < Runtime > = < pallet_contracts::Schedule
    < Runtime >> ::default();
}

#[derive_impl(pallet_contracts::config_preludes::TestDefaultConfig)]
impl pallet_contracts::Config for Runtime {
    type Currency = Balances;
    type Schedule = Schedule;
    type CallStack = [pallet_contracts::Frame<Self>; 5];
}
"#
	);

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

	// `pop up network ./network.toml --skip-confirm`
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
