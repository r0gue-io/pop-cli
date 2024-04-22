use crate::utils::{
	helpers::{get_manifest_path, parse_balance},
	signer::create_signer,
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

pub async fn dry_run_gas_estimate_instantiate(
	instantiate_exec: &InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> anyhow::Result<Weight> {
	let instantiate_result = instantiate_exec.instantiate_dry_run().await?;
	match instantiate_result.result {
		Ok(_) => {
			// use user specified values where provided, otherwise use the estimates
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
		Err(ref _err) => {
			Err(anyhow::anyhow!(
                "Pre-submission dry-run failed. Add gas_limit and proof_size manually to skip this step."
            ))
		},
	}
}

pub async fn instantiate_smart_contract(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<String, ErrorVariant> {
	let instantiate_result = instantiate_exec.instantiate(Some(gas_limit)).await?;
	Ok(instantiate_result.contract_address.to_string())
}

#[cfg(feature = "unit_contract")]
#[cfg(test)]
mod tests {
	use super::*;
	use crate::{build_smart_contract, create_smart_contract};
	use anyhow::{Error, Result};
	use std::fs;
	use tempfile::TempDir;
	use url::Url;

	const CONTRACTS_NETWORK_URL: &str = "wss://rococo-contracts-rpc.polkadot.io";

	fn generate_smart_contract_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		let result =
			create_smart_contract("test_contract".to_string(), temp_contract_dir.as_path());
		assert!(result.is_ok(), "Contract test environment setup failed");

		Ok(temp_dir)
	}
	fn build_smart_contract_test_environment(temp_dir: &TempDir) -> Result<(), Error> {
		build_smart_contract(&Some(temp_dir.path().join("test_contract")))?;
		Ok(())
	}

	#[tokio::test]
	async fn test_set_up_deployment() -> Result<(), Error> {
		let temp_dir = generate_smart_contract_test_environment()?;
		build_smart_contract_test_environment(&temp_dir)?;

		let call_opts = UpOpts {
			path: Some(temp_dir.path().join("test_contract")),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "1000".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			salt: None,
		};
		let result = set_up_deployment(call_opts).await;
		assert!(result.is_ok());
		assert_eq!(result.unwrap().url(), "wss://rococo-contracts-rpc.polkadot.io:443/");
		Ok(())
	}

	#[tokio::test]
	async fn test_dry_run_gas_estimate_instantiate() -> Result<(), Error> {
		let temp_dir = generate_smart_contract_test_environment()?;
		build_smart_contract_test_environment(&temp_dir)?;

		let call_opts = UpOpts {
			path: Some(temp_dir.path().join("test_contract")),
			constructor: "new".to_string(),
			args: ["false".to_string()].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			salt: None,
		};
		let instantiate_exec = set_up_deployment(call_opts).await;

		let result = dry_run_gas_estimate_instantiate(&instantiate_exec.unwrap()).await;
		assert!(result.is_ok());
		let weight = result.unwrap();
		assert!(weight.clone().ref_time() > 0);
		assert!(weight.proof_size() > 0);

		Ok(())
	}
}
