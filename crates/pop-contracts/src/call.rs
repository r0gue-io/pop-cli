// SPDX-License-Identifier: GPL-3.0

use crate::{
	errors::Error,
	submit_signed_payload,
	utils::{
		get_manifest_path,
		metadata::{process_function_args, FunctionType},
		parse_balance,
	},
	CallExec, DefaultEnvironment, Environment, Verbosity,
};
use anyhow::Context;
use pop_common::{create_signer, DefaultConfig, Keypair};
use std::path::PathBuf;
use subxt::{tx::Payload, SubstrateConfig};
use url::Url;
#[cfg(feature = "v5")]
use {
	contract_extrinsics::{
		extrinsic_calls::Call, BalanceVariant, CallCommandBuilder, ContractArtifacts,
		DisplayEvents, ErrorVariant, ExtrinsicOptsBuilder, TokenMetadata,
	},
	pop_common::{parse_account, Config},
	sp_weights::Weight,
};
#[cfg(feature = "v6")]
use {
	contract_extrinsics_inkv6::{
		extrinsic_calls::Call, BalanceVariant, CallCommandBuilder, ContractArtifacts,
		DisplayEvents, ErrorVariant, ExtrinsicOptsBuilder, TokenMetadata,
	},
	pop_common::account_id::parse_h160_account,
	sp_weights_inkv6::Weight,
};

/// Attributes for the `call` command.
#[derive(Clone, Debug, PartialEq)]
pub struct CallOpts {
	/// Path to the contract build directory.
	pub path: Option<PathBuf>,
	/// The address of the contract to call.
	pub contract: String,
	/// The name of the contract message to call.
	pub message: String,
	/// The message arguments, encoded as strings.
	pub args: Vec<String>,
	/// Transfers an initial balance to the instantiated contract.
	pub value: String,
	/// Maximum amount of gas to be used for this command.
	pub gas_limit: Option<u64>,
	/// Maximum proof size for the instantiation.
	pub proof_size: Option<u64>,
	/// Websocket endpoint of a node.
	pub url: Url,
	/// Secret key URI for the account deploying the contract.
	pub suri: String,
	/// Submit an extrinsic for on-chain execution.
	pub execute: bool,
}

/// Prepare the preprocessed data for a contract `call`.
///
/// # Arguments
///
/// * `call_opts` - options for the `call` command.
pub async fn set_up_call(
	call_opts: CallOpts,
) -> Result<CallExec<DefaultConfig, DefaultEnvironment, Keypair>, Error> {
	let token_metadata = TokenMetadata::query::<DefaultConfig>(&call_opts.url).await?;
	let signer = create_signer(&call_opts.suri)?;

	let extrinsic_opts = match &call_opts.path {
		// If path is a file construct the ExtrinsicOptsBuilder from the file.
		Some(path) if path.is_file() => {
			let artifacts = ContractArtifacts::from_manifest_or_file(None, Some(path))?;
			ExtrinsicOptsBuilder::new(signer)
				.file(Some(artifacts.artifact_path()))
				.url(call_opts.url.clone())
				.done()
		},
		_ => {
			let manifest_path = get_manifest_path(call_opts.path.as_deref())?;
			ExtrinsicOptsBuilder::new(signer)
				.manifest_path(Some(manifest_path))
				.url(call_opts.url.clone())
				.done()
		},
	};

	let value: BalanceVariant<<DefaultEnvironment as Environment>::Balance> =
		parse_balance(&call_opts.value)?;

	#[cfg(feature = "v5")]
	let contract: <DefaultConfig as Config>::AccountId = parse_account(&call_opts.contract)?;
	#[cfg(feature = "v6")]
	let contract = parse_h160_account(&call_opts.contract)?;
	// Process the provided argument values.
	let args = process_function_args(
		call_opts.path.unwrap_or_else(|| PathBuf::from("./")),
		&call_opts.message,
		call_opts.args,
		FunctionType::Message,
	)?;

	let call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair> =
		CallCommandBuilder::new(contract.clone(), &call_opts.message, extrinsic_opts)
			.args(args)
			.value(value.denominate_balance(&token_metadata)?)
			.gas_limit(call_opts.gas_limit)
			.proof_size(call_opts.proof_size)
			.done()
			.await?;
	Ok(call_exec)
}

/// Simulate a smart contract call without modifying the state of the blockchain.
///
/// # Arguments
///
/// * `call_exec` - struct with the call to be executed.
pub async fn dry_run_call(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> Result<String, Error> {
	let call_result = call_exec.call_dry_run().await?;
	match call_result.result {
		Ok(ref ret_val) => {
			let value = call_exec
				.transcoder()
				.decode_message_return(call_exec.message(), &mut &ret_val.data[..])
				.context(format!("Failed to decode return value {:?}", &ret_val))?;
			Ok(value.to_string())
		},
		Err(ref err) => {
			let error_variant =
				ErrorVariant::from_dispatch_error(err, &call_exec.client().metadata())?;
			Err(Error::DryRunCallContractError(format!("{error_variant}")))
		},
	}
}

/// Estimate the gas required for a contract call without modifying the state of the blockchain.
///
/// # Arguments
///
/// * `call_exec` - the preprocessed data to call a contract.
pub async fn dry_run_gas_estimate_call(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> Result<Weight, Error> {
	let call_result = call_exec.call_dry_run().await?;
	match call_result.result {
		Ok(_) => {
			// Use user specified values where provided, otherwise use the estimates.
			let ref_time =
				call_exec.gas_limit().unwrap_or_else(|| call_result.gas_required.ref_time());
			let proof_size =
				call_exec.proof_size().unwrap_or_else(|| call_result.gas_required.proof_size());
			Ok(Weight::from_parts(ref_time, proof_size))
		},
		Err(ref err) => {
			let error_variant =
				ErrorVariant::from_dispatch_error(err, &call_exec.client().metadata())?;
			Err(Error::DryRunCallContractError(format!("{error_variant}")))
		},
	}
}

/// Call a smart contract on the blockchain.
///
/// # Arguments
///
/// * `call_exec` - struct with the call to be executed.
/// * `gas_limit` - maximum amount of gas to be used for this call.
/// * `url` - endpoint of the node which to send the call to.
pub async fn call_smart_contract(
	call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
	url: &Url,
) -> anyhow::Result<String, Error> {
	let token_metadata = TokenMetadata::query::<DefaultConfig>(url).await?;
	let metadata = call_exec.client().metadata();
	#[cfg(feature = "v6")]
	let storage_deposit_limit = call_exec.opts().storage_deposit_limit();
	let events = call_exec
		.call(
			Some(gas_limit),
			#[cfg(feature = "v6")]
			storage_deposit_limit,
		)
		.await
		.map_err(|error_variant| Error::CallContractError(format!("{:?}", error_variant)))?;
	let display_events =
		DisplayEvents::from_events::<DefaultConfig, DefaultEnvironment>(&events, None, &metadata)?;

	let output =
		display_events.display_events::<DefaultEnvironment>(Verbosity::Default, &token_metadata)?;
	Ok(output)
}

/// Executes a smart contract call using a signed payload.
///
/// # Arguments
///
/// * `call_exec` - A struct containing the details of the contract call.
/// * `payload` - The signed payload string to be submitted for executing the call.
/// * `url` - The endpoint of the node where the call is executed.
pub async fn call_smart_contract_from_signed_payload(
	call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
	payload: String,
	url: &Url,
) -> anyhow::Result<String, Error> {
	let token_metadata = TokenMetadata::query::<DefaultConfig>(url).await?;
	let metadata = call_exec.client().metadata();
	let events = submit_signed_payload(url.as_str(), payload).await?;
	let display_events = DisplayEvents::from_events::<SubstrateConfig, DefaultEnvironment>(
		&events, None, &metadata,
	)?;

	let output =
		display_events.display_events::<DefaultEnvironment>(Verbosity::Default, &token_metadata)?;
	Ok(output)
}

/// Generates the payload for executing a smart contract call.
///
/// # Arguments
/// * `call_exec` - A struct containing the details of the contract call.
/// * `gas_limit` - The maximum amount of gas allocated for executing the contract call.
#[cfg(feature = "v5")]
pub fn get_call_payload(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<Vec<u8>> {
	let storage_deposit_limit: Option<u128> = call_exec.opts().storage_deposit_limit();
	let mut encoded_data = Vec::<u8>::new();
	Call::new(
		call_exec.contract().into(),
		call_exec.value(),
		gas_limit,
		storage_deposit_limit.as_ref(),
		call_exec.call_data().clone(),
	)
	.build()
	.encode_call_data_to(&call_exec.client().metadata(), &mut encoded_data)?;
	Ok(encoded_data)
}

/// Generates the payload for executing a smart contract call.
///
/// # Arguments
/// * `call_exec` - A struct containing the details of the contract call.
/// * `gas_limit` - The maximum amount of gas allocated for executing the contract call.
#[cfg(feature = "v6")]
pub fn get_call_payload(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
	storage_deposit_limit: u128,
) -> anyhow::Result<Vec<u8>> {
	let mut encoded_data = Vec::<u8>::new();
	Call::new(
		*call_exec.contract(),
		call_exec.value(),
		gas_limit,
		&storage_deposit_limit,
		call_exec.call_data().clone(),
	)
	.build()
	.encode_call_data_to(&call_exec.client().metadata(), &mut encoded_data)?;
	Ok(encoded_data)
}

#[cfg(test)]
mod tests {
	use super::*;
	#[cfg(feature = "v6")]
	use crate::AccountMapper;
	use crate::{
		contracts_node_generator, dry_run_gas_estimate_instantiate, errors::Error,
		instantiate_smart_contract, mock_build_process, new_environment, run_contracts_node,
		set_up_deployment, Bytes, UpOpts,
	};
	use anyhow::Result;
	use pop_common::{find_free_port, set_executable_permission};
	use std::{env, process::Command, time::Duration};
	use tokio::time::sleep;

	#[cfg(feature = "v5")]
	const CONTRACT_ADDRESS: &str = "5CLPm1CeUvJhZ8GCDZCR7nWZ2m3XXe4X5MtAQK69zEjut36A";
	#[cfg(feature = "v5")]
	const CONTRACTS_NETWORK_URL: &str = "wss://rpc2.paseo.popnetwork.xyz";
	#[cfg(feature = "v6")]
	const CONTRACT_ADDRESS: &str = "0x4f04054746fb19d3b027f5fe1ca5e87a68b49bac";
	#[cfg(feature = "v6")]
	const CONTRACTS_NETWORK_URL: &str = "wss://westend-asset-hub-rpc.polkadot.io";

	#[tokio::test]
	async fn test_set_up_call() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;

		let call_opts = CallOpts {
			path: Some(temp_dir.path().join("testing")),
			contract: CONTRACT_ADDRESS.to_string(),
			message: "get".to_string(),
			args: [].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			execute: false,
		};
		let call = set_up_call(call_opts).await?;
		assert_eq!(call.message(), "get");
		Ok(())
	}

	#[tokio::test]
	async fn test_set_up_call_from_artifact_file() -> Result<()> {
		let current_dir = env::current_dir().expect("Failed to get current directory");
		let call_opts = CallOpts {
			path: Some(current_dir.join("./tests/files/testing.json")),
			contract: CONTRACT_ADDRESS.to_string(),
			message: "get".to_string(),
			args: [].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			execute: false,
		};
		let call = set_up_call(call_opts).await?;
		assert_eq!(call.message(), "get");
		Ok(())
	}

	#[tokio::test]
	async fn test_set_up_call_error_contract_not_build() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let call_opts = CallOpts {
			path: Some(temp_dir.path().join("testing")),
			contract: CONTRACT_ADDRESS.to_string(),
			message: "get".to_string(),
			args: [].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			execute: false,
		};
		assert!(
			matches!(set_up_call(call_opts).await, Err(Error::AnyhowError(message)) if message.root_cause().to_string() == "Failed to find any contract artifacts in target directory. \nRun `cargo contract build --release` to generate the artifacts.")
		);
		Ok(())
	}
	#[tokio::test]
	async fn test_set_up_call_fails_no_smart_contract_directory() -> Result<()> {
		let call_opts = CallOpts {
			path: None,
			contract: CONTRACT_ADDRESS.to_string(),
			message: "get".to_string(),
			args: [].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			execute: false,
		};
		assert!(
			matches!(set_up_call(call_opts).await, Err(Error::AnyhowError(message)) if message.root_cause().to_string() == "No 'ink' dependency found")
		);
		Ok(())
	}

	#[tokio::test]
	async fn test_dry_run_call_error_contract_not_deployed() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;

		let call_opts = CallOpts {
			path: Some(temp_dir.path().join("testing")),
			contract: CONTRACT_ADDRESS.to_string(),
			message: "get".to_string(),
			args: [].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			execute: false,
		};
		let call = set_up_call(call_opts).await?;
		assert!(matches!(dry_run_call(&call).await, Err(Error::DryRunCallContractError(..))));
		Ok(())
	}

	#[tokio::test]
	async fn test_dry_run_estimate_call_error_contract_not_deployed() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;

		let call_opts = CallOpts {
			path: Some(temp_dir.path().join("testing")),
			contract: CONTRACT_ADDRESS.to_string(),
			message: "get".to_string(),
			args: [].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
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

	#[tokio::test]
	async fn call_works() -> Result<()> {
		let random_port = find_free_port(None);
		let localhost_url = format!("ws://127.0.0.1:{}", random_port);
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		#[cfg(feature = "v5")]
		let contract_file = "./tests/files/testing_wasm.contract";
		#[cfg(feature = "v6")]
		let contract_file = "./tests/files/testing.contract";
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(contract_file),
			current_dir.join("./tests/files/testing.json"),
		)?;

		let cache = temp_dir.path().join("");

		let binary = contracts_node_generator(cache.clone(), None).await?;
		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;
		let process = run_contracts_node(binary.path(), None, random_port).await?;
		// Wait 5 secs more to give time for the node to be ready
		sleep(Duration::from_millis(5000)).await;
		// Instantiate a Smart Contract.
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
		let weight = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
		let contract_info = instantiate_smart_contract(instantiate_exec, weight).await?;
		// Test querying a value.
		let query_exec = set_up_call(CallOpts {
			path: Some(temp_dir.path().join("testing")),
			contract: contract_info.address.clone(),
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
			contract: contract_info.address,
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
		// Stop the process contracts-node
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;

		Ok(())
	}
}
