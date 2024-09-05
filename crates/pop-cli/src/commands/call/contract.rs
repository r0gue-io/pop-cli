// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	style::Theme,
};
use anyhow::{anyhow, Result};
use clap::Args;
use cliclack::{clear_screen, confirm, input, intro, log, outro, outro_cancel, set_theme};
use console::style;
use pop_contracts::{
	call_smart_contract, dry_run_call, dry_run_gas_estimate_call, get_messages, set_up_call,
	CallOpts, Message,
};
use sp_weights::Weight;
use std::path::{Path, PathBuf};

#[derive(Args, Clone)]
pub struct CallContractCommand {
	/// Path to the contract build directory.
	#[arg(short = 'p', long)]
	path: Option<PathBuf>,
	/// The address of the contract to call.
	#[clap(name = "contract", long, env = "CONTRACT")]
	contract: Option<String>,
	/// The name of the contract message to call.
	#[clap(long, short)]
	message: Option<String>,
	/// The constructor arguments, encoded as strings.
	#[clap(long, num_args = 0..)]
	args: Vec<String>,
	/// Transfers an initial balance to the contract.
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
	/// Secret key URI for the account calling the contract.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(name = "suri", long, short, default_value = "//Alice")]
	suri: String,
	/// Submit an extrinsic for on-chain execution.
	#[clap(short('x'), long)]
	execute: bool,
	/// Perform a dry-run via RPC to estimate the gas usage. This does not submit a transaction.
	#[clap(long, conflicts_with = "execute")]
	dry_run: bool,
}

impl CallContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<()> {
		clear_screen()?;
		intro(format!("{}: Calling a contract", style(" Pop CLI ").black().on_magenta()))?;
		set_theme(Theme);

		let call_config = if self.contract.is_none() {
			guide_user_to_call_contract().await?
		} else {
			self.clone()
		};
		let contract = call_config
			.contract
			.expect("contract can not be none as fallback above is interactive input; qed");
		let message = call_config
			.message
			.expect("message can not be none as fallback above is interactive input; qed");

		let call_exec = set_up_call(CallOpts {
			path: call_config.path,
			contract,
			message,
			args: call_config.args,
			value: call_config.value,
			gas_limit: call_config.gas_limit,
			proof_size: call_config.proof_size,
			url: call_config.url,
			suri: call_config.suri,
			execute: call_config.execute,
		})
		.await?;

		if call_config.dry_run {
			let spinner = cliclack::spinner();
			spinner.start("Doing a dry run to estimate the gas...");
			match dry_run_gas_estimate_call(&call_exec).await {
				Ok(w) => {
					log::info(format!("Gas limit: {:?}", w))?;
					log::warning("Your call has not been executed.")?;
				},
				Err(e) => {
					spinner.error(format!("{e}"));
					outro_cancel("Call failed.")?;
				},
			};
			return Ok(());
		}

		if !call_config.execute {
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
						log::info(format!("Gas limit: {:?}", w))?;
						w
					},
					Err(e) => {
						spinner.error(format!("{e}"));
						outro_cancel("Call failed.")?;
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

/// Guide the user to call the contract.
async fn guide_user_to_call_contract() -> anyhow::Result<CallContractCommand> {
	Cli.intro("Calling a contract")?;

	// Prompt for location of your contract.
	let input_path: String = input("Path to your contract")
		.placeholder("./my_contract")
		.default_input("./")
		.interact()?;
	let contract_path = Path::new(&input_path);

	// Prompt for contract address.
	let contract_address: String = input("Paste the on-chain contract address:")
		.placeholder("5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
		.default_input("5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
		.interact()?;

	let messages = get_messages(contract_path)?;
	let message = display_select_options(&messages)?;
	let mut contract_args = Vec::new();
	for arg in &message.args {
		contract_args.push(input(arg).placeholder(arg).interact()?);
	}
	let mut value = "0".to_string();
	if message.payable {
		value = input("Value to transfer to the contract: ")
			.placeholder("0")
			.default_input("0")
			.interact()?;
	}
	// Prompt for gas limit of the call.
	let gas_limit_input: u64 = input("Gas Limit:")
		.required(false)
		.placeholder("By default it will use an Estimation")
		.default_input("0")
		.interact()?;
	let gas_limit: Option<u64> = (gas_limit_input != 0).then_some(gas_limit_input);

	// Prompt for proof_size of your contract.
	let proof_size_input: u64 = input("Proof size:")
		.required(false)
		.placeholder("By default it will use an Estimation")
		.default_input("0")
		.interact()?;
	let proof_size: Option<u64> = (proof_size_input != 0).then_some(proof_size_input);

	// Prompt for contract location.
	let url: String = input("Where is your contract?")
		.placeholder("ws://localhost:9944")
		.default_input("ws://localhost:9944")
		.interact()?;

	// Who is calling the contract.
	let suri: String = input("Secret key URI for the account calling the contract:")
		.placeholder("//Alice")
		.default_input("//Alice")
		.interact()?;

	let is_call_confirmed: bool =
		confirm("Do you want to execute the call? (Selecting 'No' will perform a dry run)")
			.initial_value(true)
			.interact()?;

	Ok(CallContractCommand {
		path: Some(contract_path.to_path_buf()),
		contract: Some(contract_address),
		message: Some(message.label.clone()),
		args: contract_args,
		value,
		gas_limit,
		proof_size,
		url: url::Url::parse(&url)?,
		suri,
		execute: message.mutates,
		dry_run: is_call_confirmed,
	})
}

fn display_select_options(messages: &Vec<Message>) -> Result<&Message> {
	let mut prompt = cliclack::select("Select the call:".to_string());
	for message in messages {
		prompt = prompt.item(message, &message.label, &message.docs);
	}
	Ok(prompt.interact()?)
}
