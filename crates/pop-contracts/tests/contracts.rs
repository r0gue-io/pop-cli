// SPDX-License-Identifier: GPL-3.0

//! Integration tests for smart contract deployment and interaction functionality.

#![cfg(feature = "integration-tests")]

use anyhow::Result;
use pop_common::{DefaultConfig, Keypair, parse_h160_account, test_env::InkTestNode};
use pop_contracts::{
	AccountMapper, CallOpts, DefaultEnvironment, Error, UpOpts, call_smart_contract, dry_run_call,
	dry_run_gas_estimate_call, dry_run_gas_estimate_instantiate, dry_run_upload, get_contract_code,
	get_upload_payload, instantiate_smart_contract, is_chain_alive, mock_build_process,
	new_environment, set_up_call, set_up_deployment, set_up_upload, upload_smart_contract,
};
use std::{env, path::PathBuf};
use tempfile::TempDir;
use url::Url;

use contract_extrinsics::{ExtrinsicOpts, ExtrinsicOptsBuilder};
use subxt_signer::sr25519::dev;

const CONTRACT_FILE: &str = "./tests/files/testing.contract";

//full_contract_lifecycle_on_local_node
#[tokio::test]
async fn run_contracts_node_works() -> Result<()> {
	let node = InkTestNode::spawn().await?;
	let localhost_url = node.ws_url();
	let local_url = url::Url::parse(localhost_url)?;

	// Check if the node is alive
	assert!(is_chain_alive(local_url).await?);

	map_account_works(localhost_url).await?;

	// Tests the deployment
	let temp_dir = new_environment("testing")?;
	let current_dir = env::current_dir().expect("Failed to get current directory");
	mock_build_process(
		temp_dir.path().join("testing"),
		current_dir.join(CONTRACT_FILE),
		current_dir.join("./tests/files/testing.json"),
	)?;
	set_up_deployment_works(&temp_dir, localhost_url).await?;
	set_up_upload_works(&temp_dir, localhost_url).await?;
	get_payload_works(&temp_dir, localhost_url).await?;
	dry_run_gas_estimate_instantiate_works(&temp_dir, localhost_url).await?;
	dry_run_gas_estimate_instantiate_throw_custom_error(&temp_dir, localhost_url).await?;
	dry_run_upload_throw_custom_error(&temp_dir, localhost_url).await?;
	let contract_address = instantiate_and_upload(&temp_dir, localhost_url).await?;

	// Tests the call of contract
	test_set_up_call(&temp_dir, localhost_url, &contract_address).await?;
	test_set_up_call_from_artifact_file(localhost_url, &contract_address).await?;
	test_set_up_call_error_contract_not_build(localhost_url, &contract_address).await?;
	test_set_up_call_fails_no_smart_contract_directory(localhost_url, &contract_address).await?;
	test_dry_run_call_error_contract_not_deployed(&temp_dir, localhost_url, &contract_address)
		.await?;
	test_dry_run_estimate_call_error_contract_not_deployed(
		&temp_dir,
		localhost_url,
		&contract_address,
	)
	.await?;
	call_works(&temp_dir, localhost_url, &contract_address).await?;

	Ok(())
}

async fn map_account_works(localhost_url: &str) -> Result<()> {
	let current_dir = env::current_dir().expect("Failed to get current directory");
	let signer = dev::alice();
	let extrinsic_opts: ExtrinsicOpts<DefaultConfig, DefaultEnvironment, Keypair> =
		ExtrinsicOptsBuilder::new(signer)
			.file(Some(current_dir.join(CONTRACT_FILE)))
			.url(Url::parse(localhost_url)?)
			.done();
	let map = AccountMapper::new(&extrinsic_opts).await?;
	assert!(map.needs_mapping().await?);

	let address = map.map_account().await?;
	assert_eq!(address, parse_h160_account("0x9621dde636de098b43efb0fa9b61facfe328f99d")?);

	assert!(!map.needs_mapping().await?);
	Ok(())
}

async fn set_up_deployment_works(temp_dir: &TempDir, localhost_url: &str) -> Result<()> {
	let up_opts = UpOpts {
		path: temp_dir.path().join("testing"),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,

		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	set_up_deployment(up_opts).await?;
	Ok(())
}

async fn set_up_upload_works(temp_dir: &TempDir, localhost_url: &str) -> Result<()> {
	let up_opts = UpOpts {
		path: temp_dir.path().join("testing"),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,

		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	set_up_upload(up_opts).await?;
	Ok(())
}

async fn get_payload_works(temp_dir: &TempDir, localhost_url: &str) -> Result<()> {
	let up_opts = UpOpts {
		path: temp_dir.path().join("testing"),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,

		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	let contract_code = get_contract_code(up_opts.path.as_ref())?;
	let call_data = {
		let upload_exec = set_up_upload(up_opts).await?;
		get_upload_payload(upload_exec, contract_code, localhost_url).await?
	};

	// Verify payload generation produces valid data:
	// 1. Payload is non-empty
	assert!(!call_data.is_empty(), "Payload should not be empty");

	// 2. Payload has reasonable size
	// A valid upload payload should include: pallet index, call index, contract code, and storage
	// deposit limit Expect at least 100 bytes for a minimal contract
	assert!(
		call_data.len() >= 100,
		"Payload size {} should be at least 100 bytes",
		call_data.len()
	);

	// 3. Verify payload is deterministic (same inputs = same output)
	let call_data_second = {
		let up_opts_second = UpOpts {
			path: temp_dir.path().join("testing"),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(localhost_url)?,
			suri: "//Alice".to_string(),
		};
		let contract_code_second = get_contract_code(up_opts_second.path.as_ref())?;
		let upload_exec_second = set_up_upload(up_opts_second).await?;
		get_upload_payload(upload_exec_second, contract_code_second, localhost_url).await?
	};
	assert_eq!(call_data, call_data_second, "Payload should be deterministic for the same inputs");

	Ok(())
}

async fn dry_run_gas_estimate_instantiate_works(
	temp_dir: &TempDir,
	localhost_url: &str,
) -> Result<()> {
	let up_opts = UpOpts {
		path: temp_dir.path().join("testing"),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,

		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	};
	let instantiate_exec = set_up_deployment(up_opts).await?;
	let weight = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
	assert!(weight.ref_time() > 0);
	assert!(weight.proof_size() > 0);
	Ok(())
}

async fn dry_run_gas_estimate_instantiate_throw_custom_error(
	temp_dir: &TempDir,
	localhost_url: &str,
) -> Result<()> {
	let up_opts = UpOpts {
		path: temp_dir.path().join("testing"),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "10000".to_string(),
		gas_limit: None,
		proof_size: None,

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
		path: temp_dir.path().join("testing"),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,

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
		path: temp_dir.path().join("testing"),
		constructor: "new".to_string(),
		args: [].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,

		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	})
	.await?;
	// Only upload a Smart Contract
	let upload_result = upload_smart_contract(&upload_exec).await?;
	assert!(!upload_result.starts_with("0x0x"));
	assert!(upload_result.starts_with("0x"));

	// Instantiate a Smart Contract
	let instantiate_exec = set_up_deployment(UpOpts {
		path: temp_dir.path().join("testing"),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
	})
	.await?;
	// First gas estimation
	let weight = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
	assert!(weight.ref_time() > 0);
	assert!(weight.proof_size() > 0);
	// Instantiate smart contract
	let contract_info = instantiate_smart_contract(instantiate_exec, weight).await?;
	assert!(contract_info.address.starts_with("0x"));
	assert!(contract_info.code_hash.is_none());

	Ok(contract_info.address)
}

async fn test_set_up_call(
	temp_dir: &TempDir,
	localhost_url: &str,
	contract_address: &str,
) -> Result<()> {
	let call_opts = CallOpts {
		path: temp_dir.path().join("testing"),
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
		path: current_dir.join("./tests/files/testing.json"),
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
		path: temp_dir.path().join("contract_not_build"),
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
		path: PathBuf::from("./"),
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

async fn test_dry_run_call_error_contract_not_deployed(
	temp_dir: &TempDir,
	localhost_url: &str,
	contract_address: &str,
) -> Result<()> {
	let call_opts = CallOpts {
		path: temp_dir.path().join("testing"),
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
		path: temp_dir.path().join("testing"),
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
		path: temp_dir.path().join("testing"),
		contract: contract_address.to_string(),
		message: "get".to_string(),
		args: [].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	})
	.await?;
	let mut query = dry_run_call(&query_exec).await?;
	assert_eq!(query, "Ok(false)");
	// Test extrinsic execution by flipping the value.
	let call_exec = set_up_call(CallOpts {
		path: temp_dir.path().join("testing"),
		contract: contract_address.to_string(),
		message: "flip".to_string(),
		args: [].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(localhost_url)?,
		suri: "//Alice".to_string(),
		execute: false,
	})
	.await?;
	let (_, weight) = dry_run_gas_estimate_call(&call_exec).await?;
	assert!(weight.ref_time() > 0);
	assert!(weight.proof_size() > 0);
	call_smart_contract(call_exec, weight, &Url::parse(localhost_url)?).await?;
	// Assert that the value has been flipped.
	query = dry_run_call(&query_exec).await?;
	assert_eq!(query, "Ok(true)");

	Ok(())
}
