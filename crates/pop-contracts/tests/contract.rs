// SPDX-License-Identifier: GPL-3.0
use anyhow::{Error, Result};
use pop_contracts::{
	build_smart_contract, create_smart_contract, dry_run_gas_estimate_instantiate,
	set_up_deployment, UpOpts,
};
use std::fs;
use tempfile::TempDir;
use url::Url;

fn setup_test_environment() -> std::result::Result<tempfile::TempDir, Error> {
	let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
	let temp_contract_dir = temp_dir.path().join("test_contract");
	fs::create_dir(&temp_contract_dir)?;
	crate::create_smart_contract("test_contract", temp_contract_dir.as_path())?;
	Ok(temp_dir)
}

#[test]
fn test_contract_build() -> std::result::Result<(), Error> {
	let temp_contract_dir = setup_test_environment()?;

	build_smart_contract(&Some(temp_contract_dir.path().join("test_contract")))?;

	// Verify that the folder target has been created
	assert!(temp_contract_dir.path().join("test_contract/target").exists());
	// Verify that all the artifacts has been generated
	assert!(temp_contract_dir
		.path()
		.join("test_contract/target/ink/test_contract.contract")
		.exists());
	assert!(temp_contract_dir
		.path()
		.join("test_contract/target/ink/test_contract.wasm")
		.exists());
	assert!(temp_contract_dir
		.path()
		.join("test_contract/target/ink/test_contract.json")
		.exists());

	Ok(())
}

const CONTRACTS_NETWORK_URL: &str = "wss://rococo-contracts-rpc.polkadot.io";

fn build_smart_contract_test_environment(temp_dir: &TempDir) -> Result<(), Error> {
	build_smart_contract(&Some(temp_dir.path().join("test_contract")))?;
	Ok(())
}

#[tokio::test]
async fn test_set_up_deployment() -> std::result::Result<(), Error> {
	let temp_dir = setup_test_environment()?;
	build_smart_contract_test_environment(&temp_dir)?;

	let call_opts = UpOpts {
		path: Some(temp_dir.path().join("test_contract")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "1000".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(CONTRACTS_NETWORK_URL)?,
		suri: "//Alice".to_string(),
		salt: None,
	};
	let result = set_up_deployment(call_opts).await?;
	assert_eq!(result.url(), "wss://rococo-contracts-rpc.polkadot.io:443/");
	Ok(())
}

#[tokio::test]
async fn test_dry_run_gas_estimate_instantiate() -> std::result::Result<(), Error> {
	let temp_dir = setup_test_environment()?;
	build_smart_contract_test_environment(&temp_dir)?;

	let call_opts = UpOpts {
		path: Some(temp_dir.path().join("test_contract")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(CONTRACTS_NETWORK_URL)?,
		suri: "//Alice".to_string(),
		salt: None,
	};
	let instantiate_exec = set_up_deployment(call_opts).await;

	let weight = dry_run_gas_estimate_instantiate(&instantiate_exec.unwrap()).await?;
	assert!(weight.clone().ref_time() > 0);
	assert!(weight.proof_size() > 0);

	Ok(())
}
