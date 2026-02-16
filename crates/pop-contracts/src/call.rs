// SPDX-License-Identifier: GPL-3.0

use crate::{
	CallExec, DefaultEnvironment, Environment, Verbosity,
	errors::Error,
	submit_signed_payload,
	utils::{
		get_manifest_path,
		metadata::{FunctionType, extract_function, process_function_args},
		parse_balance,
	},
};
use anyhow::Context;
use pop_common::{DefaultConfig, Keypair, account_id::parse_h160_account, create_signer};
use sp_weights::Weight;

use contract_extrinsics::{
	BalanceVariant, CallCommandBuilder, ContractArtifacts, DisplayEvents, ErrorVariant,
	ExtrinsicOptsBuilder, TokenMetadata, extrinsic_calls::Call,
};
use std::path::PathBuf;
use subxt::{SubstrateConfig, tx::Payload};
use url::Url;

/// Attributes for the `call` command.
#[derive(Clone, Debug, PartialEq)]
pub struct CallOpts {
	/// Path to the contract build directory.
	pub path: PathBuf,
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
	set_up_call_with_args(call_opts, None).await
}

/// Prepare the preprocessed data for a contract `call` using optional pre-processed arguments.
///
/// When `processed_args` is provided, message metadata parsing is skipped.
pub async fn set_up_call_with_args(
	call_opts: CallOpts,
	processed_args: Option<Vec<String>>,
) -> Result<CallExec<DefaultConfig, DefaultEnvironment, Keypair>, Error> {
	let signer = create_signer(&call_opts.suri)?;
	let extrinsic_opts = if call_opts.path.is_file() {
		// If path is a file construct the ExtrinsicOptsBuilder from the file.
		let artifacts = ContractArtifacts::from_manifest_or_file(None, Some(&call_opts.path))?;
		ExtrinsicOptsBuilder::new(signer)
			.file(Some(artifacts.artifact_path()))
			.url(call_opts.url.clone())
			.done()
	} else {
		let manifest_path = get_manifest_path(&call_opts.path)?;
		ExtrinsicOptsBuilder::new(signer)
			.manifest_path(Some(manifest_path))
			.url(call_opts.url.clone())
			.done()
	};

	let value: BalanceVariant<<DefaultEnvironment as Environment>::Balance> =
		parse_balance(&call_opts.value)?;
	let args = if let Some(args) = processed_args {
		args
	} else {
		// Process the provided argument values.
		let function = extract_function(call_opts.path, &call_opts.message, FunctionType::Message)?;
		process_function_args(&function, call_opts.args)?
	};
	let token_metadata = TokenMetadata::query::<DefaultConfig>(&call_opts.url).await?;
	let contract = parse_h160_account(&call_opts.contract)?;

	let call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair> =
		CallCommandBuilder::new(contract, &call_opts.message, extrinsic_opts)
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
	dry_run_gas_estimate_call(call_exec).await.map(|(value, _)| value)
}

/// Estimate the gas required for a contract call without modifying the state of the blockchain.
///
/// # Arguments
///
/// * `call_exec` - the preprocessed data to call a contract.
pub async fn dry_run_gas_estimate_call(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> Result<(String, Weight), Error> {
	let call_result = call_exec.call_dry_run().await?;
	match call_result.result {
		Ok(ref ret_val) => {
			// Use user specified values where provided, otherwise use the estimates.
			let ref_time =
				call_exec.gas_limit().unwrap_or_else(|| call_result.gas_required.ref_time());
			let proof_size =
				call_exec.proof_size().unwrap_or_else(|| call_result.gas_required.proof_size());
			let value = call_exec
				.transcoder()
				.decode_message_return(call_exec.message(), &mut &ret_val.data[..])
				.context(format!("Failed to decode return value {:?}", &ret_val))?
				.to_string();
			Ok((value, Weight::from_parts(ref_time, proof_size)))
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
	let storage_deposit_limit = call_exec.opts().storage_deposit_limit();
	let events = call_exec
		.call(Some(gas_limit), storage_deposit_limit)
		.await
		.map_err(|error_variant| Error::CallContractError(format!("{:?}", error_variant)))?;
	let display_events = DisplayEvents::from_events::<DefaultConfig, DefaultEnvironment>(
		&events,
		Some(call_exec.transcoder()),
		&metadata,
	)?;

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
