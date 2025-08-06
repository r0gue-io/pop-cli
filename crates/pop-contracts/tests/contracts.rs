// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use pop_common::find_free_port;
use pop_contracts::{
	call_smart_contract, contracts_node_generator, dry_run_call, dry_run_gas_estimate_call,
	dry_run_gas_estimate_instantiate, dry_run_upload, get_contract_code, get_upload_payload,
	instantiate_smart_contract, is_chain_alive, mock_build_process, new_environment,
	run_contracts_node, set_up_call, set_up_deployment, set_up_upload, upload_smart_contract,
	Bytes, CallOpts, Error, UpOpts,
};
#[cfg(feature = "v5")]
use sp_core::bytes::from_hex;
#[cfg(feature = "v6")]
use sp_core_inkv6::bytes::from_hex;
use std::{env, process::Command};
use subxt::{
	config::{substrate::BlakeTwo256, Hasher},
	utils::H256,
};
use tempfile::TempDir;
use url::Url;

#[cfg(feature = "v6")]
use contract_extrinsics_inkv6::{ExtrinsicOpts, ExtrinsicOptsBuilder};
#[cfg(feature = "v6")]
use pop_common::{parse_h160_account, DefaultConfig, Keypair};
#[cfg(feature = "v6")]
use pop_contracts::{AccountMapper, DefaultEnvironment};
#[cfg(feature = "v6")]
use subxt_signer::sr25519::dev;

#[cfg(feature = "v5")]
const CONTRACT_FILE: &str = "./tests/files/testing_wasm.contract";
#[cfg(feature = "v6")]
const CONTRACT_FILE: &str = "./tests/files/testing.contract";

//full_contract_lifecycle_on_local_node
#[tokio::test]
async fn run_contracts_node_works() -> Result<()> {
	// TODO: Once remove the v5, replace the way to initialize the node with:
	// let node = TestNode::spawn().await?;
	let random_port = find_free_port(None);
	let localhost_url = format!("ws://127.0.0.1:{}", random_port);
	let local_url = url::Url::parse(&localhost_url)?;

	let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
	let cache = temp_dir.path().join("");

	#[cfg(feature = "v5")]
	let version = "v0.42.0";
	#[cfg(feature = "v6")]
	let version = "v0.43.0";
	let binary = contracts_node_generator(cache.clone(), Some(version)).await?;
	binary.source(false, &(), true).await?;
	let process = run_contracts_node(binary.path(), None, random_port).await?;

	// Check if the node is alive
	assert!(is_chain_alive(local_url).await?);
	#[cfg(feature = "v5")]
	assert!(cache.join("substrate-contracts-node-v0.42.0").exists());
	#[cfg(feature = "v6")]
	assert!(cache.join("ink-node-v0.43.0").exists());
	assert!(!cache.join("artifacts").exists());

	#[cfg(feature = "v6")]
	map_account_works(&localhost_url).await?;

	// Tests the deployment
	let temp_dir = new_environment("testing")?;
	let current_dir = env::current_dir().expect("Failed to get current directory");
	mock_build_process(
		temp_dir.path().join("testing"),
		current_dir.join(CONTRACT_FILE),
		current_dir.join("./tests/files/testing.json"),
	)?;
	set_up_deployment_works(&temp_dir, &localhost_url).await?;
	set_up_upload_works(&temp_dir, &localhost_url).await?;
	get_payload_works(&temp_dir, &localhost_url).await?;
	dry_run_gas_estimate_instantiate_works(&temp_dir, &localhost_url).await?;
	#[cfg(feature = "v5")]
	dry_run_gas_estimate_instantiate_throw_custom_error(&temp_dir, &localhost_url).await?;
	dry_run_upload_throw_custom_error(&temp_dir, &localhost_url).await?;
	let contract_address = instantiate_and_upload(&temp_dir, &localhost_url).await?;

	// Tests the call of contract
	test_set_up_call(&temp_dir, &localhost_url, &contract_address).await?;
	test_set_up_call_from_artifact_file(&localhost_url, &contract_address).await?;
	test_set_up_call_error_contract_not_build(&localhost_url, &contract_address).await?;
	test_set_up_call_fails_no_smart_contract_directory(&localhost_url, &contract_address).await?;
	#[cfg(feature = "v5")]
	test_dry_run_call_error_contract_not_deployed(&temp_dir, &localhost_url, &contract_address)
		.await?;
	test_dry_run_estimate_call_error_contract_not_deployed(
		&temp_dir,
		&localhost_url,
		&contract_address,
	)
	.await?;
	call_works(&temp_dir, &localhost_url, &contract_address).await?;

	//Stop the process contracts-node
	Command::new("kill")
		.args(["-s", "TERM", &process.id().to_string()])
		.spawn()?
		.wait()?;

	Ok(())
}

#[cfg(feature = "v6")]
async fn map_account_works(localhost_url: &str) -> Result<()> {
	let current_dir = env::current_dir().expect("Failed to get current directory");
	// Alice is mapped when running the contracts-node.
	let signer = dev::bob();
	let extrinsic_opts: ExtrinsicOpts<DefaultConfig, DefaultEnvironment, Keypair> =
		ExtrinsicOptsBuilder::new(signer)
			.file(Some(current_dir.join(CONTRACT_FILE)))
			.url(Url::parse(&localhost_url)?)
			.done();
	let map = AccountMapper::new(&extrinsic_opts).await?;
	assert!(map.needs_mapping().await?);

	let address = map.map_account().await?;
	assert_eq!(address, parse_h160_account("0x41dccbd49b26c50d34355ed86ff0fa9e489d1e01")?);

	assert!(!map.needs_mapping().await?);
	Ok(())
}

async fn set_up_deployment_works(temp_dir: &TempDir, localhost_url: &str) -> Result<()> {
	let up_opts = UpOpts {
		path: Some(temp_dir.path().join("testing")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	set_up_deployment(up_opts).await?;
	Ok(())
}

async fn set_up_upload_works(temp_dir: &TempDir, localhost_url: &str) -> Result<()> {
	let up_opts = UpOpts {
		path: Some(temp_dir.path().join("testing")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	set_up_upload(up_opts).await?;
	Ok(())
}

async fn get_payload_works(temp_dir: &TempDir, localhost_url: &str) -> Result<()> {
	let up_opts = UpOpts {
		path: Some(temp_dir.path().join("testing")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	let contract_code = get_contract_code(up_opts.path.as_ref())?;
	#[cfg(feature = "v5")]
	let call_data = get_upload_payload(contract_code, localhost_url).await?;
	#[cfg(feature = "v6")]
	let call_data = {
		let upload_exec = set_up_upload(up_opts).await?;
		get_upload_payload(upload_exec, contract_code, localhost_url).await?
	};
	let payload_hash = BlakeTwo256::hash(&call_data);
	// We know that for the above opts the payload hash should be:
	// 0x33576201c216dd2a33fc05a0f1ba5c08459f232ef4a6f9bb22899ec47f8e885c
	#[cfg(feature = "v5")]
	let hex_bytes = from_hex("33576201c216dd2a33fc05a0f1ba5c08459f232ef4a6f9bb22899ec47f8e885c")
		.expect("Invalid hex string");
	#[cfg(feature = "v6")]
	let hex_bytes = from_hex("be0018c8a775f24602466cdc532b2565a140eeca9f2ff6352aa581ff0ee687a6")
		.expect("Invalid hex string");

	let hex_array: [u8; 32] = hex_bytes.try_into().expect("Expected 32-byte array");

	// Create `H256` from the `[u8; 32]` array
	let expected_hash = H256::from(hex_array);
	assert_eq!(expected_hash, payload_hash);
	Ok(())
}

async fn dry_run_gas_estimate_instantiate_works(
	temp_dir: &TempDir,
	localhost_url: &str,
) -> Result<()> {
	let up_opts = UpOpts {
		path: Some(temp_dir.path().join("testing")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	let instantiate_exec = set_up_deployment(up_opts).await?;
	let weight = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
	assert!(weight.ref_time() > 0);
	assert!(weight.proof_size() > 0);
	Ok(())
}

#[cfg(feature = "v5")]
async fn dry_run_gas_estimate_instantiate_throw_custom_error(
	temp_dir: &TempDir,
	localhost_url: &str,
) -> Result<()> {
	let up_opts = UpOpts {
		path: Some(temp_dir.path().join("testing")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "10000".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	let instantiate_exec = set_up_deployment(up_opts).await?;
	assert!(matches!(
		dry_run_gas_estimate_instantiate(&instantiate_exec).await,
		Err(Error::DryRunUploadContractError(..))
	));
	Ok(())
}

async fn dry_run_upload_throw_custom_error(temp_dir: &TempDir, localhost_url: &str) -> Result<()> {
	let up_opts = UpOpts {
		path: Some(temp_dir.path().join("testing")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	let upload_exec = set_up_upload(up_opts).await?;
	let upload_result = dry_run_upload(&upload_exec).await?;
	assert!(!upload_result.code_hash.starts_with("0x0x"));
	assert!(upload_result.code_hash.starts_with("0x"));
	Ok(())
}

async fn instantiate_and_upload(temp_dir: &TempDir, localhost_url: &str) -> Result<String> {
	let upload_exec = set_up_upload(UpOpts {
		path: Some(temp_dir.path().join("testing")),
		constructor: "new".to_string(),
		args: [].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse(&localhost_url)?,
		suri: "//Alice".to_string(),
	})
	.await?;
	// Only upload a Smart Contract
	let upload_result = upload_smart_contract(&upload_exec).await?;
	assert!(!upload_result.starts_with("0x0x"));
	assert!(upload_result.starts_with("0x"));
	// Error when Smart Contract has been already uploaded, only for ink!v5.
	#[cfg(feature = "v5")]
	assert!(matches!(
		upload_smart_contract(&upload_exec).await,
		Err(Error::UploadContractError(..))
	));

	// Instantiate a Smart Contract
	let instantiate_exec = set_up_deployment(UpOpts {
		path: Some(temp_dir.path().join("testing")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: Some(Bytes::from(vec![0x00])),
		url: Url::parse(&localhost_url)?,
		suri: "//Alice".to_string(),
	})
	.await?;
	// First gas estimation
	let weight = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
	assert!(weight.ref_time() > 0);
	assert!(weight.proof_size() > 0);
	// Instantiate smart contract
	let contract_info = instantiate_smart_contract(instantiate_exec, weight).await?;
	#[cfg(feature = "v6")]
	assert!(contract_info.address.starts_with("0x"));
	#[cfg(feature = "v5")]
	assert!(contract_info.address.starts_with("5"));
	assert!(contract_info.code_hash.is_none());

	Ok(contract_info.address)
}

async fn test_set_up_call(
	temp_dir: &TempDir,
	localhost_url: &str,
	contract_address: &str,
) -> Result<()> {
	let call_opts = CallOpts {
		path: Some(temp_dir.path().join("testing")),
		contract: contract_address.to_string(),
		message: "get".to_string(),
		args: [].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	};
	let call = set_up_call(call_opts).await?;
	assert_eq!(call.message(), "get");
	Ok(())
}

async fn test_set_up_call_from_artifact_file(
	localhost_url: &str,
	contract_address: &str,
) -> Result<()> {
	let current_dir = env::current_dir().expect("Failed to get current directory");
	let call_opts = CallOpts {
		path: Some(current_dir.join("./tests/files/testing.json")),
		contract: contract_address.to_string(),
		message: "get".to_string(),
		args: [].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	};
	let call = set_up_call(call_opts).await?;
	assert_eq!(call.message(), "get");
	Ok(())
}

async fn test_set_up_call_error_contract_not_build(
	localhost_url: &str,
	contract_address: &str,
) -> Result<()> {
	let temp_dir = new_environment("contract_not_build")?;
	let call_opts = CallOpts {
		path: Some(temp_dir.path().join("contract_not_build")),
		contract: contract_address.to_string(),
		message: "get".to_string(),
		args: [].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	};
	assert!(
		matches!(set_up_call(call_opts).await, Err(Error::AnyhowError(message)) if message.root_cause().to_string() == "Failed to find any contract artifacts in target directory. \nRun `cargo contract build --release` to generate the artifacts.")
	);
	Ok(())
}

async fn test_set_up_call_fails_no_smart_contract_directory(
	localhost_url: &str,
	contract_address: &str,
) -> Result<()> {
	let call_opts = CallOpts {
		path: None,
		contract: contract_address.to_string(),
		message: "get".to_string(),
		args: [].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	};
	assert!(
		matches!(set_up_call(call_opts).await, Err(Error::AnyhowError(message)) if message.root_cause().to_string() == "No 'ink' dependency found")
	);
	Ok(())
}

#[cfg(feature = "v5")]
async fn test_dry_run_call_error_contract_not_deployed(
	temp_dir: &TempDir,
	localhost_url: &str,
	contract_address: &str,
) -> Result<()> {
	let call_opts = CallOpts {
		path: Some(temp_dir.path().join("testing")),
		contract: contract_address.to_string(),
		message: "get".to_string(),
		args: [].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	};
	let call = set_up_call(call_opts).await?;
	assert!(matches!(dry_run_call(&call).await, Err(Error::DryRunCallContractError(..))));
	Ok(())
}

async fn test_dry_run_estimate_call_error_contract_not_deployed(
	temp_dir: &TempDir,
	localhost_url: &str,
	contract_address: &str,
) -> Result<()> {
	let call_opts = CallOpts {
		path: Some(temp_dir.path().join("testing")),
		contract: contract_address.to_string(),
		message: "get".to_string(),
		args: [].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	};
	let call = set_up_call(call_opts).await?;
	assert!(matches!(
		dry_run_gas_estimate_call(&call).await,
		Err(Error::DryRunCallContractError(..))
	));
	Ok(())
}

async fn call_works(temp_dir: &TempDir, localhost_url: &str, contract_address: &str) -> Result<()> {
	// Test querying a value.
	let query_exec = set_up_call(CallOpts {
		path: Some(temp_dir.path().join("testing")),
		contract: contract_address.to_string(),
		message: "get".to_string(),
		args: [].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(&localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	})
	.await?;
	let mut query = dry_run_call(&query_exec).await?;
	assert_eq!(query, "Ok(false)");
	// Test extrinsic execution by flipping the value.
	let call_exec = set_up_call(CallOpts {
		path: Some(temp_dir.path().join("testing")),
		contract: contract_address.to_string(),
		message: "flip".to_string(),
		args: [].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(&localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	})
	.await?;
	let weight = dry_run_gas_estimate_call(&call_exec).await?;
	assert!(weight.ref_time() > 0);
	assert!(weight.proof_size() > 0);
	call_smart_contract(call_exec, weight, &Url::parse(&localhost_url)?).await?;
	// Assert that the value has been flipped.
	query = dry_run_call(&query_exec).await?;
	assert_eq!(query, "Ok(true)");

	Ok(())
}
