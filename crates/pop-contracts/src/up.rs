// SPDX-License-Identifier: GPL-3.0

use crate::{
	Bytes, DefaultEnvironment, Environment, UploadCode, Weight,
	errors::Error,
	utils::{
		get_manifest_path,
		metadata::{FunctionType, extract_function, process_function_args},
		parse_balance,
	},
};
use contract_extrinsics::{
	BalanceVariant, Code, ContractBinary, ErrorVariant, ExtrinsicOptsBuilder,
	InstantiateCommandBuilder, InstantiateExec, InstantiateExecResult, TokenMetadata,
	UploadCommandBuilder, UploadExec, UploadResult,
	events::ContractInstantiated,
	extrinsic_calls::{Instantiate, InstantiateWithCode},
};
use pop_common::{DefaultConfig, Keypair, create_signer};
use scale_info::scale::Encode;
use sp_core::bytes::{from_hex, to_hex};
use std::{
	path::{Path, PathBuf},
	time::{SystemTime, UNIX_EPOCH},
};
use subxt::{
	OnlineClient, SubstrateConfig, backend,
	blocks::ExtrinsicEvents,
	tx::{Payload, SubmittableTransaction},
};

/// Attributes for the `up` command
#[derive(Clone, Debug, PartialEq)]
pub struct UpOpts {
	/// Path to the contract build directory.
	pub path: PathBuf,
	/// The name of the contract constructor to call.
	pub constructor: String,
	/// The constructor arguments, encoded as strings.
	pub args: Vec<String>,
	/// Transfers an initial balance to the instantiated contract.
	pub value: String,
	/// Maximum amount of gas to be used for this command.
	pub gas_limit: Option<u64>,
	/// Maximum proof size for the instantiation.
	pub proof_size: Option<u64>,
	/// Websocket endpoint of a node.
	pub url: url::Url,
	/// Secret key URI for the account deploying the contract.
	pub suri: String,
}

/// Prepare `InstantiateExec` data to upload and instantiate a contract.
///
/// # Arguments
///
/// * `up_opts` - options for the `up` command.
pub async fn set_up_deployment(
	up_opts: UpOpts,
) -> anyhow::Result<InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>> {
	set_up_deployment_with_args(up_opts, None).await
}

/// Prepare `InstantiateExec` data to upload and instantiate a contract using pre-processed args.
///
/// When `processed_args` is provided, constructor metadata parsing is skipped.
pub async fn set_up_deployment_with_args(
	up_opts: UpOpts,
	processed_args: Option<Vec<String>>,
) -> anyhow::Result<InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>> {
	let manifest_path = get_manifest_path(&up_opts.path)?;

	let token_metadata = TokenMetadata::query::<DefaultConfig>(&up_opts.url).await?;

	let signer = create_signer(&up_opts.suri)?;
	let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
		.manifest_path(Some(manifest_path))
		.url(up_opts.url.clone())
		.done();

	let value: BalanceVariant<<DefaultEnvironment as Environment>::Balance> =
		parse_balance(&up_opts.value)?;

	// Process the provided argument values.
	let args = if let Some(args) = processed_args {
		args
	} else {
		let function =
			extract_function(up_opts.path, &up_opts.constructor, FunctionType::Constructor)?;
		process_function_args(&function, up_opts.args)?
	};
	let instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair> =
		InstantiateCommandBuilder::new(extrinsic_opts)
			.constructor(up_opts.constructor.clone())
			.args(args)
			.value(value.denominate_balance(&token_metadata)?)
			.gas_limit(up_opts.gas_limit)
			.proof_size(up_opts.proof_size)
			.salt(generate_random_bytes())
			.done()
			.await?;
	Ok(instantiate_exec)
}

fn generate_random_bytes() -> Option<Bytes> {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.ok()
		.map(|time| time.as_millis().encode().into())
}

/// Prepare `UploadExec` data to upload a contract.
///
/// # Arguments
///
/// * `up_opts` - options for the `up` command.
pub async fn set_up_upload(
	up_opts: UpOpts,
) -> anyhow::Result<UploadExec<DefaultConfig, DefaultEnvironment, Keypair>> {
	let manifest_path = get_manifest_path(&up_opts.path)?;

	let signer = create_signer(&up_opts.suri)?;
	let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
		.manifest_path(Some(manifest_path))
		.url(up_opts.url.clone())
		.done();

	#[allow(unused_mut)]
	let mut upload_exec: UploadExec<DefaultConfig, DefaultEnvironment, Keypair> =
		UploadCommandBuilder::new(extrinsic_opts).done().await?;

	{
		let storage_deposit_limit = match upload_exec.opts().storage_deposit_limit() {
			Some(deposit_limit) => deposit_limit,
			None =>
				upload_exec
					.upload_code_rpc()
					.await?
					.map_err(|_| {
						Error::DryRunUploadContractError(
							"No storage limit returned from dry-run".to_string(),
						)
					})?
					.deposit,
		};
		upload_exec.set_storage_deposit_limit(Some(storage_deposit_limit));
	}

	Ok(upload_exec)
}

/// Gets the encoded payload call data for contract upload (not instantiate).
///
/// # Arguments
/// * `code` - contract code to upload.
/// * `url` - the rpc of the chain node.
pub async fn get_upload_payload(
	upload_exec: UploadExec<DefaultConfig, DefaultEnvironment, Keypair>,
	code: ContractBinary,
	url: &str,
) -> anyhow::Result<Vec<u8>> {
	let storage_deposit_limit = if let Some(limit) = upload_exec.opts().storage_deposit_limit() {
		limit
	} else {
		upload_exec
			.upload_code_rpc()
			.await?
			.map_err(|_| Error::DryRunUploadContractError("No storage limit returned".into()))?
			.deposit
	};
	let upload_code = UploadCode::new(code, storage_deposit_limit);

	let rpc_client = backend::rpc::RpcClient::from_url(url).await?;
	let client = OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client).await?;

	let call_data = upload_code.build();
	let mut encoded_data = Vec::<u8>::new();
	call_data.encode_call_data_to(&client.metadata(), &mut encoded_data)?;
	Ok(encoded_data)
}

/// Gets the encoded payload call data for a contract instantiation.
///
/// # Arguments
/// * `instantiate_exec` - arguments for contract instantiate.
/// * `gas_limit` - max amount of gas to be used for instantiation.
/// * `storage_deposit_limit` - storage deposit limit. If None, estimation will be performed.
pub async fn get_instantiate_payload(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
	storage_deposit_limit: Option<u128>,
) -> anyhow::Result<Vec<u8>> {
	let storage_deposit_limit = if let Some(limit) = storage_deposit_limit {
		limit
	} else {
		instantiate_exec.estimate_limits().await?.1
	};
	let mut encoded_data = Vec::<u8>::new();
	let args = instantiate_exec.args();
	match args.code().clone() {
		Code::Upload(code) => InstantiateWithCode::new(
			args.value(),
			gas_limit,
			storage_deposit_limit,
			code.clone(),
			args.data().into(),
			args.salt().map(|s| s.to_vec()),
		)
		.build()
		.encode_call_data_to(&instantiate_exec.client().metadata(), &mut encoded_data),
		Code::Existing(hash) => Instantiate::new(
			args.value(),
			gas_limit,
			storage_deposit_limit,
			hash,
			args.data().into(),
			args.salt().copied(),
		)
		.build()
		.encode_call_data_to(&instantiate_exec.client().metadata(), &mut encoded_data),
	}?;

	Ok(encoded_data)
}

/// Reads the contract code from contract file.
///
/// # Arguments
/// * `path` - path to the contract file.
pub fn get_contract_code(path: &Path) -> anyhow::Result<ContractBinary> {
	let manifest_path = get_manifest_path(path)?;

	// signer does not matter for this
	let signer = create_signer("//Alice")?;
	let extrinsic_opts =
		ExtrinsicOptsBuilder::<DefaultConfig, DefaultEnvironment, Keypair>::new(signer)
			.manifest_path(Some(manifest_path))
			.done();
	let artifacts = extrinsic_opts.contract_artifacts()?;

	let artifacts_path = artifacts.artifact_path().to_path_buf();
	let binary = artifacts.contract_binary;
	Ok(binary.ok_or_else(|| {
		Error::UploadContractError(format!(
			"Contract code not found from artifact file {}",
			artifacts_path.display()
		))
	})?)
}

/// Submit a pre-signed payload for uploading a contract.
///
/// # Arguments
/// * `url` - rpc for chain.
/// * `payload` - the signed payload to submit (encoded call data).
pub async fn upload_contract_signed(
	url: &str,
	payload: String,
) -> anyhow::Result<UploadResult<SubstrateConfig>> {
	let events = submit_signed_payload(url, payload).await?;
	Ok(UploadResult { events })
}

/// Submit a pre-signed payload for instantiating a contract.
///
/// # Arguments
/// * `url` - rpc for chain.
/// * `payload` - the signed payload to submit (encoded call data).
pub async fn instantiate_contract_signed(
	url: &str,
	payload: String,
) -> anyhow::Result<InstantiateExecResult<SubstrateConfig>> {
	let events = submit_signed_payload(url, payload).await?;

	let instantiated = events.find_first::<ContractInstantiated>()?.ok_or_else(|| {
		Error::InstantiateContractError("Failed to find Instantiated event".to_string())
	})?;
	let contract_address = instantiated.contract;
	let code_hash = None;

	Ok(InstantiateExecResult { events, code_hash, contract_address })
}

/// Submit a pre-signed payload.
///
/// # Arguments
/// * `url` - rpc for chain.
/// * `payload` - the signed payload to submit (encoded call data).
pub async fn submit_signed_payload(
	url: &str,
	payload: String,
) -> anyhow::Result<ExtrinsicEvents<SubstrateConfig>> {
	let rpc_client = backend::rpc::RpcClient::from_url(url).await?;
	let client = OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client).await?;

	let hex_encoded = from_hex(&payload)?;
	let extrinsic = SubmittableTransaction::from_bytes(client, hex_encoded);

	use subxt::{
		error::{RpcError, TransactionError},
		tx::TxStatus,
	};

	let mut tx = extrinsic.submit_and_watch().await?;

	while let Some(status) = tx.next().await {
		match status? {
			TxStatus::InFinalizedBlock(tx_in_block) => {
				let events = tx_in_block.wait_for_success().await?;
				return Ok(events);
			},
			TxStatus::InBestBlock(tx_in_block) => {
				let events = tx_in_block.wait_for_success().await?;
				return Ok(events);
			},
			TxStatus::Error { message } => return Err(TransactionError::Error(message).into()),
			TxStatus::Invalid { message } => return Err(TransactionError::Invalid(message).into()),
			TxStatus::Dropped { message } => return Err(TransactionError::Dropped(message).into()),
			_ => continue,
		}
	}
	Err(RpcError::SubscriptionDropped.into())
}

/// Estimate the gas required for instantiating a contract without modifying the state of the
/// blockchain.
///
/// # Arguments
/// * `instantiate_exec` - the preprocessed data to instantiate a contract.
pub async fn dry_run_gas_estimate_instantiate(
	instantiate_exec: &InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> Result<Weight, Error> {
	let instantiate_result = instantiate_exec.instantiate_dry_run().await?;
	match instantiate_result.result {
		Ok(_) => {
			// Use user specified values where provided, otherwise use the estimates.
			let ref_time = instantiate_exec
				.args()
				.gas_limit()
				.unwrap_or_else(|| instantiate_result.gas_required.ref_time());
			let proof_size = instantiate_exec
				.args()
				.proof_size()
				.unwrap_or_else(|| instantiate_result.gas_required.proof_size());
			Ok(Weight::from_parts(ref_time, proof_size))
		},
		Err(ref err) => {
			let error_variant =
				ErrorVariant::from_dispatch_error(err, &instantiate_exec.client().metadata())?;
			Err(Error::DryRunUploadContractError(format!("{error_variant}")))
		},
	}
}

/// Result of a dry-run upload of a smart contract.
pub struct UploadDryRunResult {
	/// The key under which the new code is stored.
	pub code_hash: String,
	/// The deposit that was reserved at the caller. Is zero when the code already existed.
	pub deposit: String,
}

/// Performs a dry-run for uploading a contract without modifying the state of the blockchain.
///
/// # Arguments
/// * `upload_exec` - the preprocessed data to upload a contract.
pub async fn dry_run_upload(
	upload_exec: &UploadExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> Result<UploadDryRunResult, Error> {
	match upload_exec.upload_code_rpc().await? {
		Ok(result) => {
			let upload_result = UploadDryRunResult {
				code_hash: format!("{:?}", result.code_hash),
				deposit: result.deposit.to_string(),
			};
			Ok(upload_result)
		},
		Err(ref err) => {
			let error_variant =
				ErrorVariant::from_dispatch_error(err, &upload_exec.client().metadata())?;
			Err(Error::DryRunUploadContractError(format!("{error_variant}")))
		},
	}
}

/// Type to represent information about a deployed smart contract.
pub struct ContractInfo {
	/// The on-chain address of the deployed contract.
	pub address: String,
	/// The hash of the contract's code
	pub code_hash: Option<String>,
}

/// Instantiate a contract.
///
/// # Arguments
/// * `instantiate_exec` - the preprocessed data to instantiate a contract.
/// * `gas_limit` - maximum amount of gas to be used for this call.
pub async fn instantiate_smart_contract(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<ContractInfo, Error> {
	let instantiate_result = instantiate_exec
		.instantiate(Some(gas_limit), instantiate_exec.opts().storage_deposit_limit())
		.await
		.map_err(|error_variant| Error::InstantiateContractError(format!("{:?}", error_variant)))?;
	// If is upload + instantiate, return the code hash.
	let hash = instantiate_result.code_hash.map(|code_hash| format!("{:?}", code_hash));

	let address = format!("{:?}", instantiate_result.contract_address);

	Ok(ContractInfo { address, code_hash: hash })
}

/// Upload a contract.
///
/// # Arguments
/// * `upload_exec` - the preprocessed data to upload a contract.
pub async fn upload_smart_contract(
	upload_exec: &UploadExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> anyhow::Result<String, Error> {
	#[allow(unused_variables)]
	let upload_result = upload_exec
		.upload_code()
		.await
		.map_err(|error_variant| Error::UploadContractError(format!("{:?}", error_variant)))?;

	Ok(to_hex(&upload_exec.code().code_hash(), false))
}
