// SPDX-License-Identifier: GPL-3.0

//! Integration tests for chain-related functionality.

#![cfg(all(feature = "chain", feature = "integration-tests"))]

use anyhow::Result;
use pop_chains::{
	ChainTemplate,
	up::{Binary, Source::GitHub},
};
use pop_common::{
	polkadot_sdk::sort_by_latest_semantic_version,
	pop, resolve_port,
	sourcing::{ArchiveFileSpec, GitHub::ReleaseArchive},
	templates::Template,
};
use std::{
	fs,
	fs::write,
	path::{Path, PathBuf},
	time::Duration,
};
use strum::VariantArray;
use tempfile::tempdir;
use tokio::process::Child;

/// Utility child process wrapper to kill the child process on drop.
///
/// To be used exclusively for tests.
struct TestChildProcess(pub(crate) Child);

impl Drop for TestChildProcess {
	fn drop(&mut self) {
		let _ = self.0.start_kill();
	}
}

// Test that all templates are generated correctly
#[tokio::test]
async fn generate_all_the_templates() -> Result<()> {
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
		assert!(command.spawn()?.wait().await?.success());
		assert!(temp_dir.join(parachain_name).exists());
	}
	Ok(())
}

/// Test the parachain lifecycle: new, build, up, call.
#[tokio::test]
async fn parachain_lifecycle() -> Result<()> {
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();

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
				"--with-frontend=create-dot-app",
				"--package-manager",
				"npm",
			],
		);
		assert!(command.spawn()?.wait().await?.success());
		assert!(working_dir.exists());
		assert!(working_dir.join("frontend").exists());
	}

	// Mock build process and fetch binary
	mock_build_process(&working_dir)?;
	assert!(temp_dir.join("test_parachain/target/release/wbuild/parachain-template-runtime/parachain_template_runtime.wasm").exists());
	let binary_name = fetch_runtime(&working_dir).await?;
	let binary_path = replace_mock_with_runtime(&working_dir, binary_name)?;
	assert!(binary_path.exists());

	// pop build spec --output ./target/pop/test-spec.json --para-id 2222 --type development --relay
	// paseo-local --protocol-id pop-protocol --chain local --deterministic=false
	// --default-bootnode=false
	let mut command = pop(
		&working_dir,
		[
			"build",
			"spec",
			"--output",
			"./target/pop/test-spec.json",
			"--id",
			"test-chain",
			"--para-id",
			"2222",
			"--type",
			"development",
			"--chain",
			"local_testnet",
			"--relay",
			"paseo-local",
			"--profile",
			"release",
			"--raw",
			"--genesis-state=true",
			"--genesis-code=true",
			"--protocol-id",
			"pop-protocol",
			"--deterministic=false",
			"--default-bootnode=false",
			"--skip-build",
		],
	);
	assert!(command.spawn()?.wait().await?.success());

	// Assert build files have been generated
	assert!(working_dir.join("target").exists());
	assert!(working_dir.join("target/pop/test-spec.json").exists());
	assert!(working_dir.join("target/pop/test-spec-raw.json").exists());
	assert!(working_dir.join("target/pop/genesis-code.wasm").exists());
	assert!(working_dir.join("target/pop/genesis-state").exists());

	let chain_spec_path = working_dir.join("target/pop/test-spec.json");
	let content = fs::read_to_string(&chain_spec_path).expect("Could not read file");
	// Assert custom values have been set properly
	assert!(content.contains("\"para_id\": 2222"));
	// assert!(content.contains("\"tokenDecimals\": 6"));
	// assert!(content.contains("\"tokenSymbol\": \"POP\""));
	assert!(content.contains("\"relay_chain\": \"paseo-local\""));
	assert!(content.contains("\"protocolId\": \"pop-protocol\""));
	assert!(content.contains("\"id\": \"test-chain\""));

	// Test the `pop bench` feature
	test_benchmarking(&working_dir).await?;

	// Overwrite the config file to manually set the port to test pop call parachain.
	let network_toml_path = working_dir.join("network.toml");
	fs::create_dir_all(&working_dir)?;
	let random_port = resolve_port(None);
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
default_command = "polkadot-omni-node"
chain_spec_path = "{}"

[[parachains.collators]]
name = "collator-01"
rpc_port = {random_port}
"#,
			chain_spec_path.as_os_str().to_str().unwrap(),
		),
	)?;

	// `pop up network ./network.toml --skip-confirm`
	let mut command = pop(
		&working_dir,
		["up", "network", "./network.toml", "-r", "stable2512", "--verbose", "--skip-confirm"],
	);
	let mut up = TestChildProcess(command.spawn()?);

	// Wait for the networks to initialize. Increased timeout to accommodate CI environment delays.
	let wait = Duration::from_secs(300);
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
	assert!(command.spawn()?.wait().await?.success());

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
	assert!(command.spawn()?.wait().await?.success());

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
			"--skip-confirm",
		],
	);
	assert!(command.spawn()?.wait().await?.success());

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
	assert!(command.spawn()?.wait().await?.success());

	assert!(up.0.try_wait()?.is_none(), "the process should still be running");
	// Stop the process
	up.0.kill().await?;
	up.0.wait().await?;

	Ok(())
}

async fn test_benchmarking(working_dir: &Path) -> Result<()> {
	// pop bench block --from 0 --to 1 --profile=release
	let mut command = pop(working_dir, ["bench", "block", "-y", "--from", "0", "--to", "1"]);
	assert!(command.spawn()?.wait().await?.success());
	// pop bench machine --allow-fail --profile=release
	command = pop(working_dir, ["bench", "machine", "-y", "--allow-fail"]);
	assert!(command.spawn()?.wait().await?.success());
	// pop bench overhead --runtime={runtime_path} --genesis-builder=runtime
	// --genesis-builder-preset=development --weight-path={output_path} --profile=release --warmup=1
	// --repeat=1 -y
	let runtime_path = get_mock_runtime_path();
	let temp_dir = tempdir()?;
	let output_path = temp_dir.path();
	assert!(!output_path.join("block_weights.rs").exists());
	command = pop(
		working_dir,
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
	assert!(command.spawn()?.wait().await?.success());

	// pop bench pallet --runtime={runtime_path} --genesis-builder=runtime
	// --pallets pallet_timestamp,pallet_system --extrinsic set,remark --output={output_path} -y
	// --skip-parameters
	assert!(!output_path.join("weights.rs").exists());
	assert!(!working_dir.join("pop-bench.toml").exists());
	command = pop(
		working_dir,
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
	assert!(command.spawn()?.wait().await?.success());
	// Parse weights file.
	assert!(output_path.join("weights.rs").exists());
	let content = fs::read_to_string(output_path.join("weights.rs"))?;
	let expected = [
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
		working_dir,
		[
			"bench",
			"pallet",
			&format!("--bench-file={}", working_dir.join("pop-bench.toml").display()),
			"-y",
		],
	);
	assert!(command.spawn()?.wait().await?.success());
	Ok(())
}

// Function that mocks the build process generating the target dir and release.
fn mock_build_process(temp_dir: &Path) -> Result<()> {
	// Create a target directory
	let target_dir = temp_dir.join("target");
	fs::create_dir_all(target_dir.join("release/wbuild/parachain-template-runtime"))?;
	// Create a release file
	fs::File::create(
		target_dir
			.join("release/wbuild/parachain-template-runtime/parachain_template_runtime.wasm"),
	)?;
	Ok(())
}

/// Fetch binary from GitHub releases
async fn fetch_runtime(cache: &Path) -> Result<String> {
	let name = "parachain_template_runtime.wasm";
	let contents = ["parachain_template_runtime.wasm"];
	let binary = Binary::Source {
		name: name.to_string(),
		source: GitHub(ReleaseArchive {
			owner: "r0gue-io".into(),
			repository: "base-parachain".into(),
			tag: None,
			tag_pattern: Some("polkadot-{version}".into()),
			prerelease: false,
			version_comparator: sort_by_latest_semantic_version,
			fallback: "stable2512".to_string(),
			archive: "parachain-template-runtime.tar.gz".to_string(),
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
fn replace_mock_with_runtime(temp_dir: &Path, runtime_name: String) -> Result<PathBuf> {
	let runtime_path = temp_dir.join(temp_dir.join(runtime_name));
	let content = fs::read(&runtime_path)?;
	write(
		temp_dir.join(
			"target/release/wbuild/parachain-template-runtime/parachain_template_runtime.wasm",
		),
		content,
	)?;
	Ok(runtime_path)
}

fn get_mock_runtime_path() -> PathBuf {
	let binary_path = "../../tests/runtimes/base_parachain_benchmark.wasm";
	std::env::current_dir().unwrap().join(binary_path).canonicalize().unwrap()
}
