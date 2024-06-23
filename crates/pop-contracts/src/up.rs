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
	TokenMetadata,
};
use ink_env::{DefaultEnvironment, Environment};
use sp_core::Bytes;
use sp_weights::Weight;
use std::path::PathBuf;
use subxt::PolkadotConfig as DefaultConfig;
use subxt_signer::sr25519::Keypair;

/// Attributes for the `up` command
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
/// * `up_opts` - attributes for the `up` command.
///
pub async fn set_up_deployment(
	up_opts: UpOpts,
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

/// Estimate the gas required for instantiating a contract without modifying the state of the blockchain.
///
/// # Arguments
///
/// * `instantiate_exec` - the preprocessed data to instantiate a contract.
///
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

/// Instantiate a contract.
///
/// # Arguments
///
/// * `instantiate_exec` - the preprocessed data to instantiate a contract.
/// * `gas_limit` - maximum amount of gas to be used for this call.
///
pub async fn instantiate_smart_contract(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<String, ErrorVariant> {
	let instantiate_result = instantiate_exec.instantiate(Some(gas_limit)).await?;
	Ok(instantiate_result.contract_address.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{create_smart_contract, errors::Error};
	use anyhow::Result;
	use std::{env, fs};
	use url::Url;

	const CONTRACTS_NETWORK_URL: &str = "wss://rococo-contracts-rpc.polkadot.io";

	fn generate_smart_contract_test_environment() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("testing");
		fs::create_dir(&temp_contract_dir)?;
		create_smart_contract("testing", temp_contract_dir.as_path())?;
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
	async fn test_set_up_deployment() -> Result<()> {
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
	async fn test_dry_run_gas_estimate_instantiate_throw_custom_error() -> Result<()> {
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
		let instantiate_exec = set_up_deployment(up_opts).await?;
		assert!(matches!(
			dry_run_gas_estimate_instantiate(&instantiate_exec).await,
			Err(Error::DryRunUploadContractError(..))
		));
		Ok(())
	}
}
