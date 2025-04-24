// SPDX-License-Identifier: GPL-3.0

use crate::{
	errors::Error,
	utils::{
		get_manifest_path,
		metadata::{process_function_args, FunctionType},
		parse_balance,
	},
	Bytes, DefaultEnvironment, Environment, UploadCode, Weight,
};
#[cfg(feature = "v6")]
use pop_common::account_id::parse_h160_account;
use pop_common::{create_signer, DefaultConfig, Keypair};
use std::path::{Path, PathBuf};
use subxt::{
	blocks::ExtrinsicEvents,
	tx::{Payload, SubmittableExtrinsic},
	SubstrateConfig,
};
#[cfg(feature = "v5")]
use {
	contract_extrinsics::{
		events::{CodeStored, ContractInstantiated},
		extrinsic_calls::{Instantiate, InstantiateWithCode},
		upload::Determinism,
		BalanceVariant, Code, ErrorVariant, ExtrinsicOptsBuilder, InstantiateCommandBuilder,
		InstantiateExec, InstantiateExecResult, TokenMetadata, UploadCommandBuilder, UploadExec,
		UploadResult, WasmCode as ContractBinary,
	},
	sp_core::bytes::from_hex,
	subxt::Config,
};
#[cfg(feature = "v6")]
use {
	contract_extrinsics_inkv6::{
		contract_address,
		extrinsic_calls::{Instantiate, InstantiateWithCode},
		fetch_contract_binary, BalanceVariant, Code, ContractBinary, ErrorVariant,
		ExtrinsicOptsBuilder, InstantiateCommandBuilder, InstantiateExec, InstantiateExecResult,
		TokenMetadata, UploadCommandBuilder, UploadExec, UploadResult,
	},
	sp_core_inkv6::bytes::{from_hex, to_hex},
};

/// Attributes for the `up` command
#[derive(Clone, Debug, PartialEq)]
pub struct UpOpts {
	/// Path to the contract build directory.
	pub path: Option<PathBuf>,
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
	/// A salt used in the address derivation of the new contract. Use to create multiple
	/// instances of the same contract code from the same account.
	pub salt: Option<Bytes>,
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
	let manifest_path = get_manifest_path(up_opts.path.as_deref())?;

	let token_metadata = TokenMetadata::query::<DefaultConfig>(&up_opts.url).await?;

	let signer = create_signer(&up_opts.suri)?;
	let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
		.manifest_path(Some(manifest_path))
		.url(up_opts.url.clone())
		.done();

	let value: BalanceVariant<<DefaultEnvironment as Environment>::Balance> =
		parse_balance(&up_opts.value)?;

	// Process the provided argument values.
	let args = process_function_args(
		up_opts.path.unwrap_or_else(|| PathBuf::from("./")),
		&up_opts.constructor,
		up_opts.args,
		FunctionType::Constructor,
	)?;
	let instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair> =
		InstantiateCommandBuilder::new(extrinsic_opts)
			.constructor(up_opts.constructor.clone())
			.args(args)
			.value(value.denominate_balance(&token_metadata)?)
			.gas_limit(up_opts.gas_limit)
			.proof_size(up_opts.proof_size)
			.salt(up_opts.salt.clone())
			.done()
			.await?;
	Ok(instantiate_exec)
}

/// Prepare `UploadExec` data to upload a contract.
///
/// # Arguments
///
/// * `up_opts` - options for the `up` command.
pub async fn set_up_upload(
	up_opts: UpOpts,
) -> anyhow::Result<UploadExec<DefaultConfig, DefaultEnvironment, Keypair>> {
	let manifest_path = get_manifest_path(up_opts.path.as_deref())?;

	let signer = create_signer(&up_opts.suri)?;
	let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
		.manifest_path(Some(manifest_path))
		.url(up_opts.url.clone())
		.done();

	#[allow(unused_mut)]
	let mut upload_exec: UploadExec<DefaultConfig, DefaultEnvironment, Keypair> =
		UploadCommandBuilder::new(extrinsic_opts).done().await?;

	#[cfg(feature = "v6")]
	{
		let storage_deposit_limit = match upload_exec.opts().storage_deposit_limit() {
			Some(deposit_limit) => deposit_limit,
			None =>
				upload_exec
					.upload_code_rpc()
					.await?
					.or_else(|_| {
						Err(Error::DryRunUploadContractError(
							"No storage limit returned from dry-run".to_string(),
						))
					})?
					.deposit,
		};
		upload_exec.set_storage_deposit_limit(Some(storage_deposit_limit));
	}

	Ok(upload_exec)
}

/// # Arguments
/// * `code` - contract code to upload.
/// * `url` - the rpc of the chain node.
#[cfg(feature = "v5")]
pub async fn get_upload_payload(code: ContractBinary, url: &str) -> anyhow::Result<Vec<u8>> {
	let storage_deposit_limit: Option<u128> = None;
	let upload_code = UploadCode::new(code, storage_deposit_limit, Determinism::Enforced);

	let rpc_client = subxt::backend::rpc::RpcClient::from_url(url).await?;
	let client = subxt::OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client).await?;
	let call_data = upload_code.build();
	let mut encoded_data = Vec::<u8>::new();
	call_data.encode_call_data_to(&client.metadata(), &mut encoded_data)?;
	Ok(encoded_data)
}

/// Gets the encoded payload call data for contract upload (not instantiate).
///
/// # Arguments
/// * `code` - contract code to upload.
/// * `url` - the rpc of the chain node.
#[cfg(feature = "v6")]
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
			.or_else(|_| Err(Error::DryRunUploadContractError("No storage limit returned".into())))?
			.deposit
	};
	let upload_code = UploadCode::new(code, storage_deposit_limit);

	let rpc_client = subxt::backend::rpc::RpcClient::from_url(url).await?;
	let client = subxt::OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client).await?;

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
#[cfg(feature = "v5")]
pub fn get_instantiate_payload(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<Vec<u8>> {
	let storage_deposit_limit: Option<u128> = None;
	let mut encoded_data = Vec::<u8>::new();
	let args = instantiate_exec.args();
	match args.code() {
		Code::Upload(code) => InstantiateWithCode::new(
			args.value(),
			gas_limit,
			storage_deposit_limit,
			code.clone(),
			args.data().into(),
			args.salt().into(),
		)
		.build()
		.encode_call_data_to(&instantiate_exec.client().metadata(), &mut encoded_data),
		Code::Existing(hash) => Instantiate::new(
			args.value(),
			gas_limit,
			storage_deposit_limit,
			hash,
			args.data().into(),
			args.salt().into(),
		)
		.build()
		.encode_call_data_to(&instantiate_exec.client().metadata(), &mut encoded_data),
	}?;
	Ok(encoded_data)
}

/// Gets the encoded payload call data for a contract instantiation.
///
/// # Arguments
/// * `instantiate_exec` - arguments for contract instantiate.
/// * `gas_limit` - max amount of gas to be used for instantiation.
#[cfg(feature = "v6")]
pub async fn get_instantiate_payload(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<Vec<u8>> {
	let storage_deposit_limit = instantiate_exec.estimate_limits().await?.1;
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
pub fn get_contract_code(path: Option<&PathBuf>) -> anyhow::Result<ContractBinary> {
	let manifest_path = get_manifest_path(path.map(|p| p as &Path))?;

	// signer does not matter for this
	let signer = create_signer("//Alice")?;
	let extrinsic_opts =
		ExtrinsicOptsBuilder::<DefaultConfig, DefaultEnvironment, Keypair>::new(signer)
			.manifest_path(Some(manifest_path))
			.done();
	let artifacts = extrinsic_opts.contract_artifacts()?;

	let artifacts_path = artifacts.artifact_path().to_path_buf();
	#[cfg(feature = "v5")]
	let binary = artifacts.code;
	#[cfg(feature = "v6")]
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
	#[cfg(feature = "v5")]
	{
		let code_stored = events.find_first::<CodeStored<subxt::config::substrate::H256>>()?;
		Ok(UploadResult { code_stored, events })
	}
	#[cfg(feature = "v6")]
	Ok(UploadResult { events })
}

/// Submit a pre-signed payload for instantiating a contract.
///
/// # Arguments
/// * `url` - rpc for chain.
/// * `payload` - the signed payload to submit (encoded call data).
pub async fn instantiate_contract_signed(
	#[cfg(feature = "v6")] instantiate_exec: InstantiateExec<
		DefaultConfig,
		DefaultEnvironment,
		Keypair,
	>,
	#[cfg(feature = "v6")] maybe_contract_address: Option<String>,
	url: &str,
	payload: String,
) -> anyhow::Result<InstantiateExecResult<SubstrateConfig>> {
	let events = submit_signed_payload(url, payload).await?;

	#[cfg(feature = "v5")]
	let (code_hash, contract_address) = {
		// The CodeStored event is only raised if the contract has not already been
		// uploaded.
		let code_hash = events
			.find_first::<CodeStored<subxt::config::substrate::H256>>()?
			.map(|code_stored| code_stored.code_hash);

		let instantiated = events
			.find_first::<ContractInstantiated<subxt::config::substrate::AccountId32>>()?
			.ok_or_else(|| {
				Error::InstantiateContractError("Failed to find Instantiated event".to_string())
			})?;
		(code_hash, instantiated.contract)
	};

	#[cfg(feature = "v6")]
	let (code_hash, contract_address) = {
		let contract_address = match maybe_contract_address {
			Some(addr) => parse_h160_account(&addr)?,
			None => {
				let rpc = instantiate_exec.rpc();
				let code = match instantiate_exec.args().code().clone() {
					Code::Upload(code) => code.into(),
					Code::Existing(hash) =>
						fetch_contract_binary(&instantiate_exec.client(), &rpc, &hash).await?,
				};
				let data = instantiate_exec.args().data();
				contract_address(
					instantiate_exec.client(),
					&rpc,
					instantiate_exec.opts().signer(),
					&instantiate_exec.args().salt().cloned(),
					&code[..],
					&data[..],
				)
				.await?
			},
		};
		(None, contract_address)
	};

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
	let rpc_client = subxt::backend::rpc::RpcClient::from_url(url).await?;
	let client = subxt::OnlineClient::<SubstrateConfig>::from_rpc_client(rpc_client).await?;

	let hex_encoded = from_hex(&payload)?;

	let extrinsic = SubmittableExtrinsic::from_bytes(client, hex_encoded);

	// src: https://github.com/use-ink/cargo-contract/blob/68691b9b6cdb7c6ec52ea441b3dc31fcb1ce08e0/crates/extrinsics/src/lib.rs#L143

	use subxt::{
		error::{RpcError, TransactionError},
		tx::TxStatus,
	};

	let mut tx = extrinsic.submit_and_watch().await?;

	while let Some(status) = tx.next().await {
		match status? {
			TxStatus::InFinalizedBlock(tx_in_block) => {
				let events = tx_in_block.wait_for_success().await?;
				return Ok(events)
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
		.instantiate(
			Some(gas_limit),
			#[cfg(feature = "v6")]
			instantiate_exec.opts().storage_deposit_limit(),
		)
		.await
		.map_err(|error_variant| Error::InstantiateContractError(format!("{:?}", error_variant)))?;
	// If is upload + instantiate, return the code hash.
	let hash = instantiate_result.code_hash.map(|code_hash| format!("{:?}", code_hash));

	#[cfg(feature = "v5")]
	let address = instantiate_result.contract_address.to_string();
	#[cfg(feature = "v6")]
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

	#[cfg(feature = "v5")]
	return get_code_hash_from_event(&upload_result, upload_exec.code().code_hash());

	#[cfg(feature = "v6")]
	Ok(to_hex(&upload_exec.code().code_hash(), false))
}

/// Get the code hash of a contract from the upload event.
///
/// # Arguments
/// * `upload_result` - the result of uploading the contract.
/// * `metadata_code_hash` - the code hash from the metadata Used only for error reporting.
#[cfg(feature = "v5")]
pub fn get_code_hash_from_event<C: Config>(
	upload_result: &UploadResult<C>,
	// used for error reporting
	metadata_code_hash: [u8; 32],
) -> Result<String, Error> {
	if let Some(code_stored) = upload_result.code_stored.as_ref() {
		Ok(format!("{:?}", code_stored.code_hash))
	} else {
		let code_hash: String = metadata_code_hash.iter().fold(String::new(), |mut output, b| {
			use std::fmt::Write;
			write!(output, "{:02x}", b).expect("expected to write to string");
			output
		});
		Err(Error::UploadContractError(format!(
			"This contract has already been uploaded with code hash: 0x{code_hash}"
		)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[cfg(feature = "v6")]
	use crate::AccountMapper;
	use crate::{
		contracts_node_generator, errors::Error, mock_build_process, new_environment,
		run_contracts_node,
	};
	use anyhow::Result;
	use pop_common::{find_free_port, set_executable_permission};
	use std::{env, process::Command, time::Duration};
	use subxt::{
		config::{substrate::BlakeTwo256, Hasher},
		utils::H256,
	};
	use tokio::time::sleep;
	use url::Url;

	#[cfg(feature = "v5")]
	const CONTRACT_FILE: &str = "./tests/files/testing_wasm.contract";
	#[cfg(feature = "v6")]
	const CONTRACT_FILE: &str = "./tests/files/testing.contract";
	#[cfg(feature = "v5")]
	const CONTRACTS_NETWORK_URL: &str = "wss://rpc2.paseo.popnetwork.xyz";
	#[cfg(feature = "v6")]
	const CONTRACTS_NETWORK_URL: &str = "wss://westend-asset-hub-rpc.polkadot.io";

	#[tokio::test]
	async fn set_up_deployment_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;
		let up_opts = UpOpts {
			path: Some(temp_dir.path().join("testing")),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
		};
		set_up_deployment(up_opts).await?;
		Ok(())
	}

	#[tokio::test]
	async fn set_up_upload_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;
		let up_opts = UpOpts {
			path: Some(temp_dir.path().join("testing")),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
		};
		set_up_upload(up_opts).await?;
		Ok(())
	}

	#[tokio::test]
	async fn get_payload_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;
		let up_opts = UpOpts {
			path: Some(temp_dir.path().join("testing")),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
		};
		let contract_code = get_contract_code(up_opts.path.as_ref())?;
		#[cfg(feature = "v5")]
		let call_data = get_upload_payload(contract_code, CONTRACTS_NETWORK_URL).await?;
		#[cfg(feature = "v6")]
		let call_data = {
			let upload_exec = set_up_upload(up_opts).await?;
			get_upload_payload(upload_exec, contract_code, CONTRACTS_NETWORK_URL).await?
		};
		let payload_hash = BlakeTwo256::hash(&call_data);
		// We know that for the above opts the payload hash should be:
		// 0x98c24584107b3a01d12e8e02c0bb634d15dc86123c44d186206813ede42f478d
		#[cfg(feature = "v5")]
		let hex_bytes = from_hex("98c24584107b3a01d12e8e02c0bb634d15dc86123c44d186206813ede42f478d")
			.expect("Invalid hex string");
		#[cfg(feature = "v6")]
		let hex_bytes = from_hex("1e971a1ba0f3fe41c9c162ab30bb0ab9300108ddf32c4c4cdd01adf01638a76f")
			.expect("Invalid hex string");

		let hex_array: [u8; 32] = hex_bytes.try_into().expect("Expected 32-byte array");

		// Create `H256` from the `[u8; 32]` array
		let expected_hash = H256::from(hex_array);
		assert_eq!(expected_hash, payload_hash);
		Ok(())
	}

	#[tokio::test]
	async fn dry_run_gas_estimate_instantiate_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;
		let up_opts = UpOpts {
			path: Some(temp_dir.path().join("testing")),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
		};
		let instantiate_exec = set_up_deployment(up_opts).await?;
		let weight = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
		assert!(weight.ref_time() > 0);
		assert!(weight.proof_size() > 0);
		Ok(())
	}

	#[tokio::test]
	#[cfg(feature = "v5")]
	async fn dry_run_gas_estimate_instantiate_throw_custom_error() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;
		let up_opts = UpOpts {
			path: Some(temp_dir.path().join("testing")),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "10000".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
		};
		let instantiate_exec = set_up_deployment(up_opts).await?;
		assert!(matches!(
			dry_run_gas_estimate_instantiate(&instantiate_exec).await,
			Err(Error::DryRunUploadContractError(..))
		));
		Ok(())
	}

	#[tokio::test]
	async fn dry_run_upload_throw_custom_error() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;
		let up_opts = UpOpts {
			path: Some(temp_dir.path().join("testing")),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
		};
		let upload_exec = set_up_upload(up_opts).await?;
		let upload_result = dry_run_upload(&upload_exec).await?;
		assert!(!upload_result.code_hash.starts_with("0x0x"));
		assert!(upload_result.code_hash.starts_with("0x"));
		Ok(())
	}

	#[tokio::test]
	async fn instantiate_and_upload() -> Result<()> {
		let random_port = find_free_port(None);
		let localhost_url = format!("ws://127.0.0.1:{}", random_port);
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("./tests/files/testing.json"),
		)?;

		let cache = temp_dir.path().join("");

		let binary = contracts_node_generator(cache.clone(), None).await?;
		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;
		let process = run_contracts_node(binary.path(), None, random_port).await?;
		// Wait 5 secs more to give time for the node to be ready
		sleep(Duration::from_millis(5000)).await;
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
		// Stop the process contracts-node
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;

		Ok(())
	}
}
