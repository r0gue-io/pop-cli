// SPDX-License-Identifier: GPL-3.0

use crate::{
	errors::Error,
	submit_signed_payload,
	utils::{
		get_manifest_path,
		metadata::{extract_function, process_function_args, FunctionType},
		parse_balance,
	},
	CallExec, DefaultEnvironment, Environment, Verbosity,
};
use anyhow::Context;
use pop_common::{create_signer, DefaultConfig, Keypair};
use sp_weights::Weight;
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
};
#[cfg(feature = "v6")]
use {
	contract_extrinsics_inkv6::{
		extrinsic_calls::Call, BalanceVariant, CallCommandBuilder, ContractArtifacts,
		DisplayEvents, ErrorVariant, ExtrinsicOptsBuilder, TokenMetadata,
	},
	pop_common::account_id::parse_h160_account,
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
	let function = extract_function(
		&call_opts.path.unwrap_or_else(|| PathBuf::from("./")),
		&call_opts.message,
		FunctionType::Message,
	)?;
	let args = process_function_args(&function, call_opts.args)?;

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
