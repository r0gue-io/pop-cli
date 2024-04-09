use anyhow::Context;
use contract_build::Verbosity;
use contract_extrinsics::{
	BalanceVariant, CallCommandBuilder, CallExec, DisplayEvents, ErrorVariant,
	ExtrinsicOptsBuilder, TokenMetadata,
};
use ink_env::{DefaultEnvironment, Environment};
use sp_weights::Weight;
use std::path::PathBuf;
use subxt::{Config, PolkadotConfig as DefaultConfig};
use subxt_signer::sr25519::Keypair;
use url::Url;

use crate::utils::{
	helpers::{get_manifest_path, parse_account, parse_balance},
	signer::create_signer,
};
pub struct CallOpts {
	/// Path to the contract build folder.
	pub path: Option<PathBuf>,
	/// The address of the the contract to call.
	pub contract: String,
	/// The name of the contract message to call.
	pub message: String,
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
	/// Submit an extrinsic for on-chain execution.
	pub execute: bool,
}

pub async fn set_up_call(
	call_opts: CallOpts,
) -> anyhow::Result<CallExec<DefaultConfig, DefaultEnvironment, Keypair>> {
	let token_metadata = TokenMetadata::query::<DefaultConfig>(&call_opts.url).await?;
	let manifest_path = get_manifest_path(&call_opts.path)?;

	let signer = create_signer(&call_opts.suri)?;
	let extrinsic_opts = ExtrinsicOptsBuilder::new(signer)
		.manifest_path(Some(manifest_path))
		.url(call_opts.url.clone())
		.done();

	let value: BalanceVariant<<DefaultEnvironment as Environment>::Balance> =
		parse_balance(&call_opts.value)?;

	let contract: <DefaultConfig as Config>::AccountId = parse_account(&call_opts.contract)?;

	let call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair> =
		CallCommandBuilder::new(contract.clone(), &call_opts.message, extrinsic_opts)
			.args(call_opts.args.clone())
			.value(value.denominate_balance(&token_metadata)?)
			.gas_limit(call_opts.gas_limit)
			.proof_size(call_opts.proof_size)
			.done()
			.await?;
	return Ok(call_exec);
}

pub async fn dry_run_call(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> anyhow::Result<String> {
	let call_result = call_exec.call_dry_run().await?;
	match call_result.result {
        Ok(ref ret_val) => {
            let value = call_exec
				.transcoder()
				.decode_message_return(
					call_exec.message(),
					&mut &ret_val.data[..],
				)
				.context(format!(
					"Failed to decode return value {:?}",
					&ret_val
			))?;
			Ok(value.to_string())
        }
        Err(ref _err) => {
             Err(anyhow::anyhow!(
                "Pre-submission dry-run failed. Add gas_limit and proof_size manually to skip this step."
            ))
        }
    }
}

pub async fn dry_run_gas_estimate_call(
	call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
) -> anyhow::Result<Weight> {
	let call_result = call_exec.call_dry_run().await?;
	match call_result.result {
        Ok(_) => {
            // use user specified values where provided, otherwise use the estimates
            let ref_time = call_exec
                .gas_limit()
                .unwrap_or_else(|| call_result.gas_required.ref_time());
            let proof_size = call_exec
                .proof_size()
                .unwrap_or_else(|| call_result.gas_required.proof_size());
            Ok(Weight::from_parts(ref_time, proof_size))
        }
        Err(ref _err) => {
             Err(anyhow::anyhow!(
                "Pre-submission dry-run failed. Add gas_limit and proof_size manually to skip this step."
            ))
        }
    }
}

pub async fn call_smart_contract(
	call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
	gas_limit: Weight,
	url: &Url,
) -> anyhow::Result<String, ErrorVariant> {
	let token_metadata = TokenMetadata::query::<DefaultConfig>(url).await?;
	let metadata = call_exec.client().metadata();
	let events = call_exec.call(Some(gas_limit)).await?;
	let display_events =
		DisplayEvents::from_events::<DefaultConfig, DefaultEnvironment>(&events, None, &metadata)?;

	let output =
		display_events.display_events::<DefaultEnvironment>(Verbosity::Default, &token_metadata)?;
	Ok(output)
}
