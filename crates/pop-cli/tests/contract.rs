// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use assert_cmd::Command;
use pop_common::{set_executable_permission, templates::Template};
use pop_contracts::{
	contracts_node_generator, dry_run_gas_estimate_instantiate, instantiate_smart_contract,
	run_contracts_node, set_up_deployment, Contract, UpOpts,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command as Cmd, time::Duration};
use strum::VariantArray;
use subxt::{config::DefaultExtrinsicParamsBuilder as Params, tx::Payload, utils::to_hex};
use subxt_signer::sr25519::dev;
use tokio::time::sleep;
use url::Url;

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

/// Test the contract lifecycle: new, build, up, call
#[tokio::test]
async fn contract_lifecycle() -> Result<()> {
	const DEFAULT_PORT: u16 = 9944;
	let temp = tempfile::tempdir().unwrap();
	let temp_dir = temp.path();
	//let temp_dir = Path::new("./"); //For testing locally
	// Test that all templates are generated correctly
	generate_all_the_templates(&temp_dir)?;
	// pop new contract test_contract (default)
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["new", "contract", "test_contract"])
		.assert()
		.success();
	assert!(temp_dir.join("test_contract").exists());

	// pop build --path ./test_contract --release
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["build", "--path", "./test_contract", "--release"])
		.assert()
		.success();
	// Verify that the directory target has been created
	assert!(temp_dir.join("test_contract/target").exists());
	// Verify that all the artifacts has been generated
	assert!(temp_dir.join("test_contract/target/ink/test_contract.contract").exists());
	assert!(temp_dir.join("test_contract/target/ink/test_contract.wasm").exists());
	assert!(temp_dir.join("test_contract/target/ink/test_contract.json").exists());

	let binary = contracts_node_generator(temp_dir.to_path_buf().clone(), None).await?;
	binary.source(false, &(), true).await?;
	set_executable_permission(binary.path())?;
	let process = run_contracts_node(binary.path(), None, DEFAULT_PORT).await?;

	// Only upload the contract
	// pop up contract --upload-only
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir.join("test_contract"))
		.args(&["up", "contract", "--upload-only"])
		.assert()
		.success();
	// Instantiate contract, only dry-run
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir.join("test_contract"))
		.args(&[
			"up",
			"contract",
			"--constructor",
			"new",
			"--args",
			"false",
			"--suri",
			"//Alice",
			"--dry-run",
		])
		.assert()
		.success();
	// Using methods from the pop_contracts crate to instantiate it to get the Contract Address for
	// the call
	let instantiate_exec = set_up_deployment(UpOpts {
		path: Some(temp_dir.join("test_contract")),
		constructor: "new".to_string(),
		args: ["false".to_string()].to_vec(),
		value: "0".to_string(),
		gas_limit: None,
		proof_size: None,
		salt: None,
		url: Url::parse("ws://127.0.0.1:9944")?,
		suri: "//Alice".to_string(),
	})
	.await?;
	let weight_limit = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
	let contract_info = instantiate_smart_contract(instantiate_exec, weight_limit).await?;
	// Call contract (only query)
	// pop call contract --contract $INSTANTIATED_CONTRACT_ADDRESS --message get --suri //Alice
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir.join("test_contract"))
		.args(&[
			"call",
			"contract",
			"--contract",
			&contract_info.address,
			"--message",
			"get",
			"--suri",
			"//Alice",
		])
		.assert()
		.success();

	// Call contract (execute extrinsic)
	// pop call contract --contract $INSTANTIATED_CONTRACT_ADDRESS --message flip --suri //Alice -x
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir.join("test_contract"))
		.args(&[
			"call",
			"contract",
			"--contract",
			&contract_info.address,
			"--message",
			"flip",
			"--suri",
			"//Alice",
			"-x",
		])
		.assert()
		.success();

	// Stop the process contracts-node
	Cmd::new("kill")
		.args(["-s", "TERM", &process.id().to_string()])
		.spawn()?
		.wait()?;

	Ok(())
}

#[tokio::test]
async fn wait_for_wallet_signature() -> Result<()> {
	const DEFAULT_ENDPOINT: &str = "ws://127.0.0.1:9966";
	const DEFAULT_PORT: u16 = 9966;
	const WALLET_INT_URI: &str = "http://127.0.0.1:9090";
	const WAIT_SECS: u64 = 500;
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();
	//let temp_dir = Path::new("./"); //For testing locally
	// pop new contract test_contract (default)
	Command::cargo_bin("pop")
		.unwrap()
		.current_dir(&temp_dir)
		.args(&["new", "contract", "test_contract"])
		.assert()
		.success();
	assert!(temp_dir.join("test_contract").exists());

	let binary = contracts_node_generator(temp_dir.to_path_buf().clone(), None).await?;
	binary.source(false, &(), true).await?;
	set_executable_permission(binary.path())?;
	let process = run_contracts_node(binary.path(), None, DEFAULT_PORT).await?;
	sleep(Duration::from_secs(10)).await;

	// pop up contract --upload-only --use-wallet
	// Using `cargo run --` as means for the CI to pass.
	// Possibly there's room for improvement here.
	let _handler = tokio::process::Command::new("cargo")
		.args(&[
			"run",
			"--",
			"up",
			"contract",
			"--upload-only",
			"--use-wallet",
			"--skip-confirm",
			"--dry-run",
			"--url",
			DEFAULT_ENDPOINT,
			"-p",
			temp_dir.join("test_contract").to_str().expect("to_str"),
		])
		.spawn()?;
	// Wait a moment for node and server to be up.
	sleep(Duration::from_secs(WAIT_SECS)).await;

	// Request payload from server.
	let response = reqwest::get(&format!("{}/payload", WALLET_INT_URI))
		.await
		.expect("Failed to get payload")
		.json::<TransactionData>()
		.await
		.expect("Failed to parse payload");
	// We have received some payload.
	assert!(!response.call_data().is_empty());

	let rpc_client = subxt::backend::rpc::RpcClient::from_url(DEFAULT_ENDPOINT).await?;
	let client = subxt::OnlineClient::<subxt::SubstrateConfig>::from_rpc_client(rpc_client).await?;

	// Sign payload.
	let signer = dev::alice();
	let payload = CallData(response.call_data());
	let ext_params = Params::new().build();
	let signed = client.tx().create_signed(&payload, &signer, ext_params).await?;

	// Submit signed payload. This kills the wallet integration server.
	let _ = reqwest::Client::new()
		.post(&format!("{}/submit", WALLET_INT_URI))
		.json(&to_hex(signed.encoded()))
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

	// Stop the process contracts-node
	Cmd::new("kill")
		.args(["-s", "TERM", &process.id().to_string()])
		.spawn()?
		.wait()?;

	Ok(())
}

fn generate_all_the_templates(temp_dir: &Path) -> Result<()> {
	for template in Contract::VARIANTS {
		let contract_name = format!("test_contract_{}", template).replace("-", "_");
		let contract_type = template.template_type()?.to_lowercase();
		// pop new parachain test_parachain
		Command::cargo_bin("pop")
			.unwrap()
			.current_dir(&temp_dir)
			.args(&[
				"new",
				"contract",
				&contract_name,
				"--contract-type",
				&contract_type,
				"--template",
				&template.to_string(),
			])
			.assert()
			.success();
		assert!(temp_dir.join(contract_name).exists());
	}
	Ok(())
}
