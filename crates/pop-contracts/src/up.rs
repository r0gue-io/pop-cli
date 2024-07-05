// SPDX-License-Identifier: GPL-3.0
use crate::{
	errors::Error,
	utils::{
		helpers::{get_manifest_path, parse_balance},
		signer::create_signer,
	},
};
use contract_extrinsics::{
	BalanceVariant, ErrorVariant, ExtrinsicOptsBuilder, InstantiateCommandBuilder, InstantiateExec,
	TokenMetadata, UploadCommandBuilder, UploadExec,
};
use ink_env::{DefaultEnvironment, Environment};
use sp_core::Bytes;
use sp_weights::Weight;
use std::path::PathBuf;
use subxt::PolkadotConfig as DefaultConfig;
use subxt_signer::sr25519::Keypair;

/// Attributes for the `up` command
#[derive(Debug, PartialEq)]
pub struct UpOpts {
	/// Path to the contract build folder.
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

	let instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair> =
		InstantiateCommandBuilder::new(extrinsic_opts)
			.constructor(up_opts.constructor.clone())
			.args(up_opts.args.clone())
			.value(value.denominate_balance(&token_metadata)?)
			.gas_limit(up_opts.gas_limit)
			.proof_size(up_opts.proof_size)
			.salt(up_opts.salt.clone())
			.done()
			.await?;
	return Ok(instantiate_exec);
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

	let upload_exec: UploadExec<DefaultConfig, DefaultEnvironment, Keypair> =
		UploadCommandBuilder::new(extrinsic_opts).done().await?;
	return Ok(upload_exec);
}

/// Estimate the gas required for instantiating a contract without modifying the state of the blockchain.
///
/// # Arguments
///
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
	pub code_hash: String,
	pub deposit: String,
}

/// Performs a dry-run for uploading a contract without modifying the state of the blockchain.
///
/// # Arguments
///
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

/// Instantiate a contract.
///
/// # Arguments
///
/// * `instantiate_exec` - the preprocessed data to instantiate a contract.
/// * `gas_limit` - maximum amount of gas to be used for this call.
pub async fn instantiate_smart_contract(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<String, Error> {
	let instantiate_result = instantiate_exec
		.instantiate(Some(gas_limit))
		.await
		.map_err(|error_variant| Error::InstantiateContractError(format!("{:?}", error_variant)))?;
	Ok(instantiate_result.contract_address.to_string())
}

/// Upload a contract.
///
/// # Arguments
///
/// * `upload_exec` - the preprocessed data to upload a contract.
pub async fn upload_smart_contract(
	upload_exec: &UploadExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> anyhow::Result<String, Error> {
	let upload_result = upload_exec
		.upload_code()
		.await
		.map_err(|error_variant| Error::UploadContractError(format!("{:?}", error_variant)))?;
	if let Some(code_stored) = upload_result.code_stored {
		return Ok(format!("0x{:?}", code_stored.code_hash));
	} else {
		let code_hash: String =
			upload_exec.code().code_hash().iter().map(|b| format!("{:02x}", b)).collect();
		Err(Error::UploadContractError(format!(
			"This contract has already been uploaded with code hash: 0x{code_hash}"
		)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{create_smart_contract, errors::Error, run_contracts_node, templates::Contract};
	use anyhow::Result;
	use std::{env, fs, process::Command};
	use url::Url;

	const CONTRACTS_NETWORK_URL: &str = "wss://rpc2.paseo.popnetwork.xyz";

	fn generate_smart_contract_test_environment() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("testing");
		fs::create_dir(&temp_contract_dir)?;
		create_smart_contract("testing", temp_contract_dir.as_path(), &Contract::Standard)?;
		Ok(temp_dir)
	}
	// Function that mocks the build process generating the contract artifacts.
	fn mock_build_process(temp_contract_dir: PathBuf) -> Result<(), Error> {
		// Create a target directory
		let target_contract_dir = temp_contract_dir.join("target");
		fs::create_dir(&target_contract_dir)?;
		fs::create_dir(&target_contract_dir.join("ink"))?;
		// Copy a mocked testing.contract file inside the target directory
		let current_dir = env::current_dir().expect("Failed to get current directory");
		let contract_file = current_dir.join("tests/files/testing.contract");
		fs::copy(contract_file, &target_contract_dir.join("ink/testing.contract"))?;
		Ok(())
	}

	#[tokio::test]
	async fn set_up_deployment_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;
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
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;
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
	async fn dry_run_gas_estimate_instantiate_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;
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
	async fn dry_run_gas_estimate_instantiate_throw_custom_error() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;
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
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;
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
		assert!(upload_result.code_hash.starts_with("0x"));
		Ok(())
	}

	#[tokio::test]
	async fn instantiate_and_upload() -> Result<()> {
		const LOCALHOST_URL: &str = "ws://127.0.0.1:9944";
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;
		// Run contracts-node
		let cache = temp_dir.path().join("cache");
		let process = run_contracts_node(cache).await?;

		let upload_exec = set_up_upload(UpOpts {
			path: Some(temp_dir.path().join("testing")),
			constructor: "new".to_string(),
			args: [].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			salt: None,
			url: Url::parse(LOCALHOST_URL)?,
			suri: "//Alice".to_string(),
		})
		.await?;

		// Only upload a Smart Contract
		let upload_result = upload_smart_contract(&upload_exec).await?;
		assert!(upload_result.starts_with("0x"));
		//Error when Smart Contract has been already uploaded
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
			url: Url::parse(LOCALHOST_URL)?,
			suri: "//Alice".to_string(),
		})
		.await?;
		// First gas estimation
		let weight = dry_run_gas_estimate_instantiate(&instantiate_exec).await?;
		assert!(weight.ref_time() > 0);
		assert!(weight.proof_size() > 0);
		// Instantiate smart contract
		let address = instantiate_smart_contract(instantiate_exec, weight).await?;
		assert!(address.starts_with("5"));
		// Stop the process contracts-node
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;
		Ok(())
	}
}
