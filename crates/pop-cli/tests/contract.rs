// SPDX-License-Identifier: GPL-3.0

//! Contract integration tests for validating contract lifecycle functionality.

#![cfg(feature = "contract")]

use anyhow::Result;
use common::{cleanup_telemetry_env, pop, MockTelemetry};
use pop_common::{find_free_port, set_executable_permission, templates::Template};
use pop_contracts::{
	contracts_node_generator, dry_run_call, dry_run_gas_estimate_call,
	dry_run_gas_estimate_instantiate, instantiate_smart_contract, run_contracts_node, set_up_call,
	set_up_deployment, CallOpts, UpOpts, Weight,
};
use serde::{Deserialize, Serialize};
use std::{process::Command as Cmd, time::Duration};
use strum::{EnumMessage, VariantArray};
use subxt::{config::DefaultExtrinsicParamsBuilder as Params, tx::Payload, utils::to_hex};
use subxt_signer::sr25519::dev;
use tokio::time::sleep;
use url::Url;

mod common;

// This struct implements the [`Payload`] trait and is used to submit
// pre-encoded SCALE call data directly, without the dynamic construction of transactions.
struct CallData(Vec<u8>);
impl Payload for CallData {
	fn encode_call_data_to(
		&self,
		_: &subxt::Metadata,
		out: &mut Vec<u8>,
	) -> Result<(), subxt::ext::subxt_core::Error> {
		out.extend_from_slice(&self.0);
		Ok(())
	}
}

// TransactionData has been copied from wallet_integration.rs
/// Transaction payload to be sent to frontend for signing.
#[derive(Serialize, Debug)]
#[cfg_attr(test, derive(Deserialize, Clone))]
pub struct TransactionData {
	chain_rpc: String,
	call_data: Vec<u8>,
}
impl TransactionData {
	pub fn new(chain_rpc: String, call_data: Vec<u8>) -> Self {
		Self { chain_rpc, call_data }
	}
	pub fn call_data(&self) -> Vec<u8> {
		self.call_data.clone()
	}
}

#[tokio::test]
async fn generate_all_contract_templates() -> Result<()> {
	// Setup wiremock server and telemetry environment
	let telemetry = MockTelemetry::new().await?;

	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();

	for template in pop_contracts::Contract::VARIANTS {
		let contract_name = format!("test_contract_{}", template).replace("-", "_");
		let contract_type = template.template_type()?.to_lowercase();

		let mut command = pop(
			temp_dir,
			[
				"new",
				"contract",
				&contract_name,
				"--contract-type",
				&contract_type,
				"--template",
				&template.to_string(),
			],
		);
		assert!(command.spawn()?.wait()?.success());
		telemetry
			.assert_latest_payload_structure("new contract", template.get_message().unwrap_or(""))
			.await?;
		assert!(temp_dir.join(&contract_name).exists());
	}

	cleanup_telemetry_env();
	Ok(())
}

// SubmitRequest has been copied from wallet_integration.rs
/// Payload submitted by the wallet after signing a transaction.
#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct SubmitRequest {
	/// Signed transaction returned from the wallet.
	pub signed_payload: Option<String>,
	/// Address of the deployed contract, included only when the transaction is a contract
	/// deployment.
	pub contract_address: Option<String>,
}

/// Test contract creation and building
#[tokio::test]
async fn test_contract_creation_and_build() -> Result<()> {
	let telemetry = MockTelemetry::new().await?;
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();

	// pop new contract test_contract (default)
	let mut command = pop(temp_dir, ["new", "contract", "test_contract"]);
	assert!(command.spawn()?.wait()?.success());
	telemetry.assert_latest_payload_structure("new contract", "Standard").await?;
	assert!(temp_dir.join("test_contract").exists());

	// pop build --path ./test_contract --release
	let mut command = pop(temp_dir, ["build", "--path", "./test_contract", "--release"]);
	assert!(command.spawn()?.wait()?.success());
	telemetry.assert_latest_payload_structure("build", "contract").await?;

	// Verify that the directory target has been created
	assert!(temp_dir.join("test_contract/target").exists());
	// Verify that all the artifacts has been generated
	assert!(temp_dir.join("test_contract/target/ink/test_contract.contract").exists());
	#[cfg(feature = "wasm-contracts")]
	assert!(temp_dir.join("test_contract/target/ink/test_contract.wasm").exists());
	#[cfg(feature = "polkavm-contracts")]
	assert!(temp_dir.join("test_contract/target/ink/test_contract.polkavm").exists());
	assert!(temp_dir.join("test_contract/target/ink/test_contract.json").exists());

	cleanup_telemetry_env();
	Ok(())
}

/// Test contract upload and instantiation - requires built contract.
#[tokio::test]
async fn test_contract_upload_and_instantiation() -> Result<()> {
	let telemetry = MockTelemetry::new().await?;
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();
	let endpoint_port = find_free_port(None);
	let default_endpoint: &str = &format!("ws://127.0.0.1:{}", endpoint_port);

	// Setup: create and build the contract
	let mut command = pop(temp_dir, ["new", "contract", "test_contract"]);
	assert!(command.spawn()?.wait()?.success());
	let mut command = pop(temp_dir, ["build", "--path", "./test_contract", "--release"]);
	assert!(command.spawn()?.wait()?.success());

	// Start contracts node
	let binary = contracts_node_generator(temp_dir.to_path_buf().clone(), None).await?;
	binary.source(false, &(), true).await?;
	set_executable_permission(binary.path())?;
	let process = run_contracts_node(binary.path(), None, endpoint_port).await?;
	sleep(Duration::from_secs(5)).await;

	// Upload the contract
	let mut command = pop(
		temp_dir,
		["up", "--path", "./test_contract", "--upload-only", "--url", default_endpoint],
	);
	assert!(command.spawn()?.wait()?.success());
	telemetry.assert_latest_payload_structure("up", "contract").await?;

	// Test instantiation dry-run
	let mut command = pop(
		&temp_dir.join("test_contract"),
		[
			"up",
			"--constructor",
			"new",
			"--args",
			"false",
			"--suri",
			"//Alice",
			"--dry-run",
			"--url",
			default_endpoint,
		],
	);
	assert!(command.spawn()?.wait()?.success());
	telemetry.assert_latest_payload_structure("up", "contract").await?;

	// Cleanup
	Cmd::new("kill")
		.args(["-s", "TERM", &process.id().to_string()])
		.spawn()?
		.wait()?;

	cleanup_telemetry_env();
	Ok(())
}

/// Test contract calls.
#[tokio::test]
async fn test_contract_calls() -> Result<()> {
	let telemetry = MockTelemetry::new().await?;
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();
	let endpoint_port = find_free_port(None);
	let default_endpoint: &str = &format!("ws://127.0.0.1:{}", endpoint_port);

	// Setup: create, build, upload, instantiate contract
	let mut command = pop(temp_dir, ["new", "contract", "test_contract"]);
	assert!(command.spawn()?.wait()?.success());
	let mut command = pop(temp_dir, ["build", "--path", "./test_contract", "--release"]);
	assert!(command.spawn()?.wait()?.success());

	let binary = contracts_node_generator(temp_dir.to_path_buf().clone(), None).await?;
	binary.source(false, &(), true).await?;
	set_executable_permission(binary.path())?;
	let process = run_contracts_node(binary.path(), None, endpoint_port).await?;
	sleep(Duration::from_secs(5)).await;

	let mut command = pop(
		temp_dir,
		["up", "--path", "./test_contract", "--upload-only", "--url", default_endpoint],
	);
	assert!(command.spawn()?.wait()?.success());

	let instantiate_exec = set_up_deployment(UpOpts {
		path: Some(temp_dir.join("test_contract")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse(default_endpoint)?,
		suri: "//Alice".to_string(),
	})
	.await?;
	let weight_limit = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
	let contract_info = instantiate_smart_contract(instantiate_exec, weight_limit).await?;

	// Test dry-run calls
	let call_opts = CallOpts {
		path: Some(temp_dir.join("test_contract")),
		contract: contract_info.address.clone(),
		message: "get".to_string(),
		args: vec![],
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(default_endpoint)?,
		suri: "//Alice".to_string(),
		execute: false,
	};
	let call_exec = set_up_call(call_opts).await?;
	let weight_limit = dry_run_gas_estimate_call(&call_exec).await?;
	assert!(weight_limit.all_gt(Weight::zero()));
	assert_eq!(dry_run_call(&call_exec).await?, "Ok(false)");

	// Test query call via pop
	let mut command = pop(
		&temp_dir.join("test_contract"),
		[
			"call",
			"contract",
			"--contract",
			&contract_info.address,
			"--message",
			"get",
			"--suri",
			"//Alice",
			"--url",
			default_endpoint,
		],
	);
	assert!(command.spawn()?.wait()?.success());
	telemetry.assert_latest_payload_structure("call contract", "").await?;

	// Test execute call via pop
	let mut command = pop(
		&temp_dir.join("test_contract"),
		[
			"call",
			"contract",
			"--contract",
			&contract_info.address,
			"--message",
			"flip",
			"--suri",
			"//Alice",
			"-x",
			"--url",
			default_endpoint,
		],
	);
	assert!(command.spawn()?.wait()?.success());
	telemetry.assert_latest_payload_structure("call contract", "").await?;

	// Verify state changed
	assert_eq!(dry_run_call(&call_exec).await?, "Ok(true)");

	// Cleanup
	Cmd::new("kill")
		.args(["-s", "TERM", &process.id().to_string()])
		.spawn()?
		.wait()?;

	cleanup_telemetry_env();
	Ok(())
}

/// Test wallet integration - requires working contract setup.
#[tokio::test]
async fn test_wallet_integration() -> Result<()> {
	const WALLET_INT_URI: &str = "http://127.0.0.1:9090";
	const WAIT_SECS: u64 = 10 * 60;
	let endpoint_port = find_free_port(None);
	let default_endpoint: &str = &format!("ws://127.0.0.1:{}", endpoint_port);
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();

	let telemetry = MockTelemetry::new().await?;

	// Setup: create, build, upload contract
	let mut command = pop(temp_dir, ["new", "contract", "test_contract"]);
	assert!(command.spawn()?.wait()?.success());
	let mut command = pop(temp_dir, ["build", "--path", "./test_contract", "--release"]);
	assert!(command.spawn()?.wait()?.success());

	let binary = contracts_node_generator(temp_dir.to_path_buf().clone(), None).await?;
	binary.source(false, &(), true).await?;
	set_executable_permission(binary.path())?;
	let process = run_contracts_node(binary.path(), None, endpoint_port).await?;
	sleep(Duration::from_secs(5)).await;

	let mut command = pop(
		temp_dir,
		["up", "--path", "./test_contract", "--upload-only", "--url", default_endpoint],
	);
	assert!(command.spawn()?.wait()?.success());

	// Start wallet integration server
	let _ = tokio::process::Command::new("cargo")
		.args(&[
			"run",
			"--",
			"up",
			"--upload-only",
			"--use-wallet",
			"--skip-confirm",
			"--dry-run",
			"--path",
			temp_dir.join("test_contract").to_str().expect("to_str"),
			"--url",
			default_endpoint,
		])
		.spawn()?;
	sleep(Duration::from_secs(WAIT_SECS)).await;

	// Test wallet integration flow
	let response = reqwest::get(&format!("{}/payload", WALLET_INT_URI))
		.await
		.expect("Failed to get payload")
		.json::<TransactionData>()
		.await
		.expect("Failed to parse payload");
	assert!(!response.call_data().is_empty());

	let rpc_client = subxt::backend::rpc::RpcClient::from_url(default_endpoint).await?;
	let client = subxt::OnlineClient::<subxt::SubstrateConfig>::from_rpc_client(rpc_client).await?;

	let signer = dev::alice();
	let payload = CallData(response.call_data());
	let ext_params = Params::new().build();
	let signed = client.tx().create_signed(&payload, &signer, ext_params).await?;

	let submit_request =
		SubmitRequest { signed_payload: Some(to_hex(signed.encoded())), contract_address: None };
	let _ = reqwest::Client::new()
		.post(&format!("{}/submit", WALLET_INT_URI))
		.json(&submit_request)
		.send()
		.await
		.expect("Failed to submit payload")
		.text()
		.await
		.expect("Failed to parse JSON response");

	// Verify server shuts down
	let response = reqwest::get(&format!("{}/payload", WALLET_INT_URI)).await;
	assert!(response.is_err());

	// Cleanup
	Cmd::new("kill")
		.args(["-s", "TERM", &process.id().to_string()])
		.spawn()?
		.wait()?;

	cleanup_telemetry_env();
	Ok(())
}
