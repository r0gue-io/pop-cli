// SPDX-License-Identifier: GPL-3.0
use crate::{
	errors::Error,
	utils::{
		get_manifest_path,
		metadata::{process_function_args, FunctionType},
		parse_balance,
	},
};
use contract_extrinsics::{
	BalanceVariant, ErrorVariant, ExtrinsicOptsBuilder, InstantiateCommandBuilder, InstantiateExec,
	TokenMetadata, UploadCommandBuilder, UploadExec,
};
use ink_env::{DefaultEnvironment, Environment};
use pop_common::{create_signer, DefaultConfig, Keypair};
use sp_core::Bytes;
use sp_weights::Weight;
use std::{fmt::Write, path::PathBuf};

/// Attributes for the `up` command
#[derive(Debug, PartialEq)]
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

	let upload_exec: UploadExec<DefaultConfig, DefaultEnvironment, Keypair> =
		UploadCommandBuilder::new(extrinsic_opts).done().await?;
	Ok(upload_exec)
}

/// Estimate the gas required for instantiating a contract without modifying the state of the
/// blockchain.
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
///
/// * `instantiate_exec` - the preprocessed data to instantiate a contract.
/// * `gas_limit` - maximum amount of gas to be used for this call.
pub async fn instantiate_smart_contract(
	instantiate_exec: InstantiateExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
) -> anyhow::Result<ContractInfo, Error> {
	let instantiate_result = instantiate_exec
		.instantiate(Some(gas_limit))
		.await
		.map_err(|error_variant| Error::InstantiateContractError(format!("{:?}", error_variant)))?;
	// If is upload + instantiate, return the code hash.
	let hash = instantiate_result.code_hash.map(|code_hash| format!("{:?}", code_hash));

	Ok(ContractInfo { address: instantiate_result.contract_address.to_string(), code_hash: hash })
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
		Ok(format!("{:?}", code_stored.code_hash))
	} else {
		let code_hash: String =
			upload_exec.code().code_hash().iter().fold(String::new(), |mut output, b| {
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
	use crate::{
		contracts_node_generator, errors::Error, mock_build_process, new_environment,
		run_contracts_node,
	};
	use anyhow::Result;
	use pop_common::{find_free_port, set_executable_permission};
	use std::{env, process::Command, time::Duration};
	use tokio::time::sleep;
	use url::Url;

	const CONTRACTS_NETWORK_URL: &str = "wss://rpc2.paseo.popnetwork.xyz";

	#[tokio::test]
	async fn set_up_deployment_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
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
			current_dir.join("./tests/files/testing.contract"),
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
	async fn dry_run_gas_estimate_instantiate_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
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
	async fn dry_run_gas_estimate_instantiate_throw_custom_error() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
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
			current_dir.join("./tests/files/testing.contract"),
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
		let random_port = find_free_port();
		let localhost_url = format!("ws://127.0.0.1:{}", random_port);
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
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
