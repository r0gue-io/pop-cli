// SPDX-License-Identifier: GPL-3.0

//! Contract integration tests for validating contract lifecycle functionality.

#![cfg(all(feature = "contract", feature = "integration-tests"))]

use anyhow::Result;
use pop_common::{pop, resolve_port, set_executable_permission};
use pop_contracts::{
	CallOpts, Contract, UpOpts, Weight, dry_run_call, dry_run_gas_estimate_call,
	dry_run_gas_estimate_instantiate, ink_node_generator, instantiate_smart_contract, run_ink_node,
	set_up_call, set_up_deployment,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, process::Command, time::Duration};
use strum::VariantArray;
use subxt::{
	Metadata, OnlineClient, SubstrateConfig, backend::rpc::RpcClient,
	config::DefaultExtrinsicParamsBuilder as Params, ext::subxt_core, tx::Payload, utils::to_hex,
};
use subxt_signer::sr25519::dev;
use tokio::time::sleep;
use url::Url;

// This struct implements the [`Payload`] trait and is used to submit
// pre-encoded SCALE call data directly, without the dynamic construction of transactions.
struct CallData(Vec<u8>);
impl Payload for CallData {
	fn encode_call_data_to(
		&self,
		_: &Metadata,
		out: &mut Vec<u8>,
	) -> Result<(), subxt_core::Error> {
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
	/// Create a new TransactionData instance with the given chain RPC and call data.
	pub fn new(chain_rpc: String, call_data: Vec<u8>) -> Self {
		Self { chain_rpc, call_data }
	}

	/// Get the call data.
	pub fn call_data(&self) -> Vec<u8> {
		self.call_data.clone()
	}
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

/// Test the contract lifecycle: new, build, up, call
#[tokio::test]
async fn contract_lifecycle() -> Result<()> {
	const WALLET_INT_URI: &str = "http://127.0.0.1:9090";
	let endpoint_port = resolve_port(None);
	let default_endpoint: &str = &format!("ws://127.0.0.1:{}", endpoint_port);
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();
	// Test that all templates are generated correctly
	generate_all_the_templates(temp_dir).await?;
	// pop new contract test_contract_lifecycle (default)
	let mut command = pop(
		temp_dir,
		[
			"new",
			"contract",
			"test_contract_lifecycle",
			"--template",
			"standard",
			"--with-frontend=typink",
			"--package-manager",
			"npm",
		],
	);
	assert!(command.spawn()?.wait().await?.success());
	assert!(temp_dir.join("test_contract_lifecycle").exists());
	assert!(temp_dir.join("test_contract_lifecycle").join("frontend").exists());

	// pop build --path ./test_contract_lifecycle --release
	command = pop(temp_dir, ["build", "--path", "./test_contract_lifecycle", "--release"]);
	assert!(command.spawn()?.wait().await?.success());

	// Verify that the directory target has been created
	assert!(temp_dir.join("test_contract_lifecycle/target").exists());
	// Verify that all the artifacts has been generated
	assert!(
		temp_dir
			.join("test_contract_lifecycle/target/ink/test_contract_lifecycle.contract")
			.exists()
	);
	assert!(
		temp_dir
			.join("test_contract_lifecycle/target/ink/test_contract_lifecycle.polkavm")
			.exists()
	);
	assert!(
		temp_dir
			.join("test_contract_lifecycle/target/ink/test_contract_lifecycle.json")
			.exists()
	);

	let binary = ink_node_generator(temp_dir.to_path_buf().clone(), None).await?;
	binary.source(false, &(), true).await?;
	set_executable_permission(binary.path())?;
	let mut process = run_ink_node(&binary.path(), None, endpoint_port).await?;
	sleep(Duration::from_secs(5)).await;

	// pop test --path ./test_contract_lifecycle
	command = pop(temp_dir, ["test", "--path", "./test_contract_lifecycle"]);
	assert!(command.spawn()?.wait().await?.success());
	// Only upload the contract
	// pop up --path ./test_contract_lifecycle --upload-only --suri //Alice
	command = pop(
		temp_dir,
		[
			"up",
			"--path",
			"./test_contract_lifecycle",
			"--upload-only",
			"--url",
			default_endpoint,
			"--suri",
			"//Alice",
		],
	);
	assert!(command.spawn()?.wait().await?.success());
	// Instantiate contract, only dry-run
	command = pop(
		&temp_dir.join("test_contract_lifecycle"),
		[
			"up",
			"--constructor",
			"new",
			"--args",
			"false",
			"--suri",
			"//Alice",
			"--url",
			default_endpoint,
			"--skip-confirm",
		],
	);
	assert!(command.spawn()?.wait().await?.success());

	// Using methods from the pop_contracts crate to instantiate it to get the Contract Address for
	// the call
	let instantiate_exec = set_up_deployment(UpOpts {
		path: temp_dir.join("test_contract_lifecycle"),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(default_endpoint)?,
		suri: "//Alice".to_string(),
	})
	.await?;
	let weight_limit = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
	let contract_info = instantiate_smart_contract(instantiate_exec, weight_limit).await?;

	// Dry runs
	let call_opts = CallOpts {
		path: temp_dir.join("test_contract_lifecycle"),
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
	let (_, weight_limit) = dry_run_gas_estimate_call(&call_exec).await?;
	assert!(weight_limit.all_gt(Weight::zero()));
	assert_eq!(dry_run_call(&call_exec).await?, "Ok(false)");

	// Call contract (only query)
	// pop call contract --contract $INSTANTIATED_CONTRACT_ADDRESS --message get --suri //Alice
	command = pop(
		&temp_dir.join("test_contract_lifecycle"),
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
			"--skip-confirm",
		],
	);
	assert!(command.spawn()?.wait().await?.success());

	// Call contract (execute extrinsic)
	// pop call contract --contract $INSTANTIATED_CONTRACT_ADDRESS --message flip --suri //Alice -x
	command = pop(
		&temp_dir.join("test_contract_lifecycle"),
		[
			"call",
			"contract",
			"--skip-confirm", // skip execution confirmation
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
	assert!(command.spawn()?.wait().await?.success());

	// Dry runs after changing the value
	assert_eq!(dry_run_call(&call_exec).await?, "Ok(true)");

	// pop up --upload-only --use-wallet
	// Will run http server for wallet integration.
	pop(
		&temp_dir.join("test_contract_lifecycle"),
		["up", "--upload-only", "--use-wallet", "--url", default_endpoint, "--skip-confirm"],
	)
	.spawn()?;
	// Wait a moment for node and server to be up.
	sleep(Duration::from_secs(120)).await;

	// Request payload from server.
	let response = reqwest::get(&format!("{}/payload", WALLET_INT_URI))
		.await
		.expect("Failed to get payload")
		.json::<TransactionData>()
		.await
		.expect("Failed to parse payload");
	// We have received some payload.
	assert!(!response.call_data().is_empty());

	let rpc_client = RpcClient::from_url(default_endpoint).await?;
	let client = OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client).await?;

	// Sign payload.
	let signer = dev::alice();
	let payload = CallData(response.call_data());
	let ext_params = Params::new().build();
	let signed = client.tx().create_signed(&payload, &signer, ext_params).await?;

	let signed_payload = Some(to_hex(signed.encoded()));
	// Submit signed payload. This kills the wallet integration server.
	let _ = reqwest::Client::new()
		.post(format!("{}/submit", WALLET_INT_URI))
		.json(&signed_payload)
		.send()
		.await
		.expect("Failed to submit payload")
		.text()
		.await
		.expect("Failed to parse JSON response");

	// Request payload from server after signed payload has been sent.
	// Server should not be running!
	let response = reqwest::get(&format!("{}/payload", WALLET_INT_URI)).await;
	assert!(response.is_err());

	// Stop the process ink-node
	process.kill()?;

	Ok(())
}

fn docker_available() -> bool {
	Command::new("docker")
		.arg("info")
		.output()
		.map(|output| output.status.success())
		.unwrap_or(false)
}

#[tokio::test]
async fn verifiable_contract_lifecycle() -> Result<()> {
	if !docker_available() {
		eprintln!("Skipping verifiable_contract_lifecycle: Docker is not available.");
		return Ok(());
	}

	let endpoint_port = resolve_port(None);
	let default_endpoint: &str = &format!("ws://127.0.0.1:{}", endpoint_port);

	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();

	// pop new contract test_contract (default)
	let mut command = pop(temp_dir, ["new", "contract", "test_contract", "--template", "standard"]);
	assert!(command.spawn()?.wait().await?.success());
	let contract_dir = temp_dir.join("test_contract");
	assert!(contract_dir.exists());

	// pop build --verifiable
	command = pop(temp_dir, ["build", "--path", "./test_contract", "--verifiable"]);
	assert!(command.spawn()?.wait().await?.success());

	let ink_target_path = contract_dir.join("target").join("ink");
	assert!(ink_target_path.join("test_contract.contract").exists());
	assert!(ink_target_path.join("test_contract.polkavm").exists());
	let metadata_path = ink_target_path.join("test_contract.json");
	assert!(metadata_path.exists());
	let metadata_contents: Value = serde_json::from_str(&std::fs::read_to_string(&metadata_path)?)?;
	// Verifiable builds include the used image (useink/contracts-verifiable:{tag} if not custom
	// specified) in the metadata so that they can be exactly reproduced
	let image_key = metadata_contents.get("image");
	let build_image = match image_key {
		Some(Value::String(value)) if value.starts_with("useink/contracts-verifiable") =>
			value.clone(),
		_ => return Err(anyhow::anyhow!("Verifiable build doesn't include the expected image")),
	};

	// Verify the contract recently built against itself
	command = pop(
		temp_dir,
		[
			"verify",
			"--contract-path",
			"./test_contract/target/ink/test_contract.contract",
			"--path",
			"./test_contract",
		],
	);
	assert!(command.spawn()?.wait().await?.success());

	// Create a different contract and observe that verification fails
	command = pop(temp_dir, ["new", "contract", "test_contract_2", "--template", "erc20"]);
	assert!(command.spawn()?.wait().await?.success());

	command = pop(
		temp_dir,
		[
			"verify",
			"--contract-path",
			"./test_contract/target/ink/test_contract.contract",
			"--path",
			"./test_contract_2",
		],
	);
	assert!(!command.spawn()?.wait().await?.success());

	// Spawn a local node and verify against a live deployment
	let binary = ink_node_generator(temp_dir.to_path_buf().clone(), None).await?;
	binary.source(false, &(), true).await?;
	set_executable_permission(binary.path())?;
	let mut process = run_ink_node(&binary.path(), None, endpoint_port).await?;
	sleep(Duration::from_secs(5)).await;

	// Deploy the verifiable contract to the local node
	let instantiate_exec = set_up_deployment(UpOpts {
		path: contract_dir.clone(),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		url: Url::parse(default_endpoint)?,
		suri: "//Alice".to_string(),
	})
	.await?;
	let weight_limit = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
	let contract_info = instantiate_smart_contract(instantiate_exec, weight_limit).await?;

	// Wait for the contract to be finalized in storage
	sleep(Duration::from_secs(10)).await;

	// Verify the contract against the live deployment using CLI
	command = pop(
		temp_dir,
		[
			"verify",
			"--path",
			"./test_contract",
			"--url",
			default_endpoint,
			"--address",
			&contract_info.address,
			"--image",
			&build_image,
		],
	);
	assert!(command.spawn()?.wait().await?.success());

	// Stop the ink-node process
	process.kill()?;

	Ok(())
}

async fn generate_all_the_templates(temp_dir: &Path) -> Result<()> {
	for template in Contract::VARIANTS {
		let contract_name = format!("test_contract_{}", template).replace("-", "_");
		// pop new contract test_contract
		let mut command =
			pop(temp_dir, ["new", "contract", &contract_name, "--template", template.as_ref()]);
		assert!(command.spawn()?.wait().await?.success());
	}
	Ok(())
}
