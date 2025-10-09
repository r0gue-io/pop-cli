// SPDX-License-Identifier: GPL-3.0

//! Integration tests for chain-related functionality.

#![cfg(all(feature = "chain", feature = "integration-tests"))]

use anyhow::Result;
use pop_chains::{
	ChainTemplate,
	up::{Binary, Source::GitHub},
};
use pop_common::{
	find_free_port,
	polkadot_sdk::sort_by_latest_semantic_version,
	pop, set_executable_permission,
	sourcing::{ArchiveFileSpec, GitHub::ReleaseArchive},
	target,
	templates::Template,
};
use std::{
	fs,
	fs::write,
	path::{Path, PathBuf},
	process::Command,
	time::Duration,
};
use strum::VariantArray;
use tempfile::tempdir;

// Test that all templates are generated correctly
#[test]
fn generate_all_the_templates() -> Result<()> {
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();

	for template in ChainTemplate::VARIANTS {
		let parachain_name = format!("test_parachain_{}", template);
		let provider = template.template_type()?.to_lowercase();
		// pop new chain test_parachain --verify
		let mut command = pop(
			temp_dir,
			[
				"new",
				"chain",
				&parachain_name,
				&provider,
				"--template",
				template.as_ref(),
				"--verify",
			],
		);
		assert!(command.spawn()?.wait()?.success());
		assert!(temp_dir.join(parachain_name).exists());
	}
	Ok(())
}

/// Test the parachain lifecycle: new, build, up, call.
#[tokio::test]
async fn parachain_lifecycle() -> Result<()> {
	// For testing locally: set to `true`
	const LOCAL_TESTING: bool = false;

	let temp = tempfile::tempdir()?;
	let temp_dir = match LOCAL_TESTING {
		true => Path::new("./"),
		false => temp.path(),
	};

	// pop new chain test_parachain --verify (default)
	let working_dir = temp_dir.join("test_parachain");
	if !working_dir.exists() {
		let mut command = pop(
			temp_dir,
			[
				"new",
				"chain",
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

	// Mock build process and fetch binary
	mock_build_process(&working_dir)?;
	assert!(temp_dir.join("test_parachain/target/release").exists());
	let binary_name = fetch_binary(&working_dir).await?;
	let binary_path = replace_mock_with_binary(&working_dir, binary_name)?;
	assert!(binary_path.exists());

	// pop build spec --output ./target/pop/test-spec.json --id 2222 --type development --relay
	// paseo-local --protocol-id pop-protocol --chain local --deterministic=true
	let mut command = pop(
		&working_dir,
		[
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
			"--genesis-state=true",
			"--genesis-code=true",
			"--protocol-id",
			"pop-protocol",
			"--deterministic=false",
			"--default-bootnode=false",
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
	// assert!(content.contains("\"tokenDecimals\": 6"));
	// assert!(content.contains("\"tokenSymbol\": \"POP\""));
	assert!(content.contains("\"relay_chain\": \"paseo-local\""));
	assert!(content.contains("\"protocolId\": \"pop-protocol\""));
	assert!(content.contains("\"id\": \"local_testnet\""));

	// Test the `pop bench` feature
	test_benchmarking(&working_dir)?;

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
		["up", "network", "./network.toml", "-r", "stable2412", "--verbose", "--skip-confirm"],
	);
	let mut up = command.spawn()?;

	// Wait for the networks to initialize. Increased timeout to accommodate CI environment delays.
	let wait = Duration::from_secs(50);
	println!("waiting for {wait:?} for network to initialize...");
	tokio::time::sleep(wait).await;

	// `pop call chain --pallet System --function remark --args "0x11" --url
	// ws://127.0.0.1:random_port --suri //Alice --skip-confirm`
	let mut command = pop(
		&working_dir,
		[
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

	// `pop call chain --pallet System --function Account --args
	// "15oF4uVJwmo4TdGW7VfQxNLavjCXviqxT9S1MgbjMNHr6Sp5" --url ws://127.0.0.1:random_port
	// --skip-confirm`
	let mut command = pop(
		&working_dir,
		[
			"call",
			"chain",
			"--pallet",
			"System",
			"--function",
			"Account",
			"--args",
			"15oF4uVJwmo4TdGW7VfQxNLavjCXviqxT9S1MgbjMNHr6Sp5",
			"--url",
			&localhost_url,
			"--skip-confirm",
		],
	);
	assert!(command.spawn()?.wait()?.success());

	// `pop call chain --pallet System --function Account --args
	// "15oF4uVJwmo4TdGW7VfQxNLavjCXviqxT9S1MgbjMNHr6Sp5" --url ws://127.0.0.1:random_port`
	let mut command = pop(
		&working_dir,
		[
			"call",
			"chain",
			"--pallet",
			"System",
			"--function",
			"Ss58Prefix",
			"--url",
			&localhost_url,
		],
	);
	assert!(command.spawn()?.wait()?.success());

	// pop call chain --call 0x00000411 --url ws://127.0.0.1:random_port --suri //Alice
	// --skip-confirm
	let mut command = pop(
		&working_dir,
		[
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
	up.kill()?;
	up.wait()?;
	Command::new("kill").args(["-s", "SIGINT", &up.id().to_string()]).spawn()?;

	Ok(())
}

fn test_benchmarking(working_dir: &Path) -> Result<()> {
	// pop bench block --from 0 --to 1 --profile=release
	let mut command =
		pop(&working_dir, ["bench", "block", "--from", "0", "--to", "1", "--profile=release"]);
	assert!(command.spawn()?.wait()?.success());
	// pop bench machine --allow-fail --profile=release
	command = pop(&working_dir, ["bench", "machine", "--allow-fail", "--profile=release"]);
	assert!(command.spawn()?.wait()?.success());
	// pop bench overhead --runtime={runtime_path} --genesis-builder=runtime
	// --genesis-builder-preset=development --weight-path={output_path} --profile=release --warmup=1
	// --repeat=1 -y
	let runtime_path = get_mock_runtime_path();
	let temp_dir = tempdir()?;
	let output_path = temp_dir.path();
	assert!(!output_path.join("block_weights.rs").exists());
	command = pop(
		&working_dir,
		[
			"bench",
			"overhead",
			&format!("--runtime={}", runtime_path.display()),
			"--genesis-builder=runtime",
			"--genesis-builder-preset=development",
			&format!("--weight-path={}", output_path.display()),
			"--warmup=1",
			"--repeat=1",
			"--profile=release",
			"-y",
		],
	);
	assert!(command.spawn()?.wait()?.success());

	// pop bench pallet --runtime={runtime_path} --genesis-builder=runtime
	// --pallets pallet_timestamp,pallet_system --extrinsic set,remark --output={output_path} -y
	// --skip-parameters
	assert!(!output_path.join("weights.rs").exists());
	assert!(!working_dir.join("pop-bench.toml").exists());
	command = pop(
		&working_dir,
		[
			"bench",
			"pallet",
			&format!("--runtime={}", runtime_path.display()),
			"--genesis-builder=runtime",
			"--pallets",
			"pallet_timestamp,pallet_system",
			"--extrinsic",
			"set,remark",
			&format!("--output={}", output_path.join("weights.rs").display()),
			"--skip-parameters",
			"-y",
		],
	);
	assert!(command.spawn()?.wait()?.success());
	// Parse weights file.
	assert!(output_path.join("weights.rs").exists());
	let content = fs::read_to_string(&output_path.join("weights.rs"))?;
	let expected = vec![
		"// Executed Command:".to_string(),
		"//  pop".to_string(),
		"//  bench".to_string(),
		"//  pallet".to_string(),
		format!("//  --runtime={}", runtime_path.display()),
		"//  --pallets=pallet_timestamp,pallet_system".to_string(),
		"//  --extrinsic=set,remark".to_string(),
		"//  --steps=50".to_string(),
		format!("//  --output={}", output_path.join("weights.rs").display()),
		"//  --genesis-builder=runtime".to_string(),
		"//  --skip-parameters".to_string(),
		"//  -y".to_string(),
	]
	.join("\n");

	assert!(
		content.contains(&expected),
		"expected command block not found.\nExpected:\n{}\n---\nContent:\n{}",
		expected,
		content
	);

	assert!(working_dir.join("pop-bench.toml").exists());
	// Use the generated pop-bench.toml file:
	// pop bench pallet --bench-file={working_dir.join("pop-bench.toml")} -y
	command = pop(
		&working_dir,
		[
			"bench",
			"pallet",
			&format!("--bench-file={}", working_dir.join("pop-bench.toml").display()),
			"-y",
		],
	);
	assert!(command.spawn()?.wait()?.success());
	Ok(())
}

// Function that mocks the build process generating the target dir and release.
fn mock_build_process(temp_dir: &Path) -> Result<()> {
	// Create a target directory
	let target_dir = temp_dir.join("target");
	fs::create_dir(&target_dir)?;
	fs::create_dir(target_dir.join("release"))?;
	// Create a release file
	fs::File::create(target_dir.join("release/parachain-template-node"))?;
	Ok(())
}

/// Fetch binary from GitHub releases
async fn fetch_binary(cache: &Path) -> Result<String> {
	let name = "parachain-template-node";
	let contents = ["parachain-template-node"];

	let binary = Binary::Source {
		name: name.to_string(),
		source: GitHub(ReleaseArchive {
			owner: "r0gue-io".into(),
			repository: "base-parachain".into(),
			tag: None,
			tag_pattern: Some("polkadot-{version}".into()),
			prerelease: false,
			version_comparator: sort_by_latest_semantic_version,
			fallback: "stable2503".to_string(),
			archive: format!("{name}-{}.tar.gz", target()?),
			contents: contents
				.into_iter()
				.map(|b| ArchiveFileSpec::new(b.into(), None, true))
				.collect(),
			latest: None,
		})
		.into(),
		cache: cache.to_path_buf(),
	};
	binary.source(true, &(), true).await?;
	Ok(name.to_string())
}

// Replace the binary fetched with the mocked binary
fn replace_mock_with_binary(temp_dir: &Path, binary_name: String) -> Result<PathBuf> {
	let binary_path = temp_dir.join(binary_name);
	let content = fs::read(&binary_path)?;
	write(temp_dir.join("target/release/parachain-template-node"), content)?;
	// Make executable
	set_executable_permission(temp_dir.join("target/release/parachain-template-node"))?;
	Ok(binary_path)
}

fn get_mock_runtime_path() -> PathBuf {
	let binary_path = "../../tests/runtimes/base_parachain_benchmark.wasm";
	std::env::current_dir().unwrap().join(binary_path).canonicalize().unwrap()
}
