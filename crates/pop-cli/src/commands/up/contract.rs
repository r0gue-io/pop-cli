// SPDX-License-Identifier: GPL-3.0

use anyhow::anyhow;
use clap::Args;
use cliclack::{clear_screen, intro, log, outro, outro_cancel};
use pop_contracts::{
	dry_run_gas_estimate_instantiate, instantiate_smart_contract, parse_hex_bytes,
	set_up_deployment, UpOpts,
};
use sp_core::Bytes;
use sp_weights::Weight;
use std::path::PathBuf;

use crate::style::style;

#[derive(Args)]
pub struct UpContractCommand {
	/// Path to the contract build folder.
	#[arg(short = 'p', long)]
	path: Option<PathBuf>,
	/// The name of the contract constructor to call.
	#[clap(name = "constructor", long, default_value = "new")]
	constructor: String,
	/// The constructor arguments, encoded as strings.
	#[clap(long, num_args = 0..)]
	args: Vec<String>,
	/// Transfers an initial balance to the instantiated contract.
	#[clap(name = "value", long, default_value = "0")]
	value: String,
	/// Maximum amount of gas to be used for this command.
	/// If not specified it will perform a dry-run to estimate the gas consumed for the
	/// instantiation.
	#[clap(name = "gas", long)]
	gas_limit: Option<u64>,
	/// Maximum proof size for the instantiation.
	/// If not specified it will perform a dry-run to estimate the proof size required.
	#[clap(long)]
	proof_size: Option<u64>,
	/// A salt used in the address derivation of the new contract. Use to create multiple
	/// instances of the same contract code from the same account.
	#[clap(long, value_parser = parse_hex_bytes)]
	salt: Option<Bytes>,
	/// Websocket endpoint of a node.
	#[clap(name = "url", long, value_parser, default_value = "ws://localhost:9944")]
	url: url::Url,
	/// Secret key URI for the account deploying the contract.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(name = "suri", long, short)]
	suri: String,
}
impl UpContractCommand {
	pub(crate) async fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Deploy a smart contract", style(" Pop CLI ").black().on_magenta()))?;
		let instantiate_exec = set_up_deployment(UpOpts {
			path: self.path.clone(),
			constructor: self.constructor.clone(),
			args: self.args.clone(),
			value: self.value.clone(),
			gas_limit: self.gas_limit,
			proof_size: self.proof_size,
			salt: self.salt.clone(),
			url: self.url.clone(),
			suri: self.suri.clone(),
		})
		.await?;

		let weight_limit;
		if self.gas_limit.is_some() && self.proof_size.is_some() {
			weight_limit = Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap());
		} else {
			let spinner = cliclack::spinner();
			spinner.start("Doing a dry run to estimate the gas...");
			weight_limit = match dry_run_gas_estimate_instantiate(&instantiate_exec).await {
				Ok(w) => {
					log::info(format!("Gas limit {:?}", w))?;
					w
				},
				Err(e) => {
					spinner.error(format!("{e}"));
					outro_cancel("Deployment failed.")?;
					return Ok(());
				},
			};
		}
		let spinner = cliclack::spinner();
		spinner.start("Uploading and instantiating the contract...");
		let contract_address = instantiate_smart_contract(instantiate_exec, weight_limit)
			.await
			.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;
		spinner.stop(format!(
			"Contract deployed and instantiated: The Contract Address is {:?}",
			contract_address
		));
		outro("Deployment complete")?;
		Ok(())
	}
}
