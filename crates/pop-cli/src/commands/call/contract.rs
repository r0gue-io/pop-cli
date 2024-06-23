// SPDX-License-Identifier: GPL-3.0

use crate::style::Theme;
use anyhow::anyhow;
use clap::Args;
use cliclack::{clear_screen, intro, log, outro, outro_cancel, set_theme};
use console::style;
use pop_contracts::{
	call_smart_contract, dry_run_call, dry_run_gas_estimate_call, set_up_call, CallOpts,
};
use sp_weights::Weight;
use std::path::PathBuf;

#[derive(Args)]
pub struct CallContractCommand {
	/// Path to the contract build folder.
	#[arg(short = 'p', long)]
	path: Option<PathBuf>,
	/// The address of the contract to call.
	#[clap(name = "contract", long, env = "CONTRACT")]
	contract: String,
	/// The name of the contract message to call.
	#[clap(long, short)]
	message: String,
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
	/// Maximum proof size for this command.
	/// If not specified it will perform a dry-run to estimate the proof size required.
	#[clap(long)]
	proof_size: Option<u64>,
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
	/// Submit an extrinsic for on-chain execution.
	#[clap(short('x'), long)]
	execute: bool,
}

impl CallContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!("{}: Calling a contract", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		let call_exec = set_up_call(CallOpts {
			path: self.path.clone(),
			contract: self.contract.clone(),
			message: self.message.clone(),
			args: self.args.clone(),
			value: self.value.clone(),
			gas_limit: self.gas_limit,
			proof_size: self.proof_size,
			url: self.url.clone(),
			suri: self.suri.clone(),
			execute: self.execute,
		})
		.await?;

		if !self.execute {
			let spinner = cliclack::spinner();
			spinner.start("Calling the contract...");
			let call_dry_run_result = dry_run_call(&call_exec).await?;
			log::info(format!("Result: {}", call_dry_run_result))?;
			log::warning("Your call has not been executed.")?;
			log::warning(format!(
                    "To submit the transaction and execute the call on chain, add {} flag to the command.",
                    "-x/--execute"
            ))?;
		} else {
			let weight_limit;
			if self.gas_limit.is_some() && self.proof_size.is_some() {
				weight_limit =
					Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap());
			} else {
				let spinner = cliclack::spinner();
				spinner.start("Doing a dry run to estimate the gas...");
				weight_limit = match dry_run_gas_estimate_call(&call_exec).await {
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
			spinner.start("Calling the contract...");

			let call_result = call_smart_contract(call_exec, weight_limit, &self.url)
				.await
				.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;

			log::info(call_result)?;
		}

		outro("Call completed successfully!")?;
		Ok(())
	}
}
