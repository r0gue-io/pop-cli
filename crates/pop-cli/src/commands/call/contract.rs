// SPDX-License-Identifier: GPL-3.0

use crate::cli::{
	traits::{Cli as _, Confirm, Input, Select},
	Cli,
};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_contracts::{
	call_smart_contract, dry_run_call, dry_run_gas_estimate_call, get_messages, parse_account,
	set_up_call, CallOpts, Message,
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
	pub(crate) async fn execute(self: Box<Self>) -> Result<()> {
		Cli.intro("Calling a contract")?;

		let call_config = if self.contract.is_none() {
			guide_user_to_call_contract().await?
		} else {
			*self.clone()
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
					Cli.info(format!("Gas limit: {:?}", w))?;
					Cli.warning("Your call has not been executed.")?;
				},
				Err(e) => {
					spinner.error(format!("{e}"));
					Cli.outro_cancel("Call failed.")?;
				},
			};
			return Ok(());
		}

		if !call_config.execute {
			let spinner = cliclack::spinner();
			spinner.start("Calling the contract...");
			let call_dry_run_result = dry_run_call(&call_exec).await?;
			Cli.info(format!("Result: {}", call_dry_run_result))?;
			Cli.warning("Your call has not been executed.")?;
		} else {
			let weight_limit = if self.gas_limit.is_some() && self.proof_size.is_some() {
				Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
			} else {
				let spinner = cliclack::spinner();
				spinner.start("Doing a dry run to estimate the gas...");
				match dry_run_gas_estimate_call(&call_exec).await {
					Ok(w) => {
						Cli.info(format!("Gas limit: {:?}", w))?;
						w
					},
					Err(e) => {
						spinner.error(format!("{e}"));
						Cli.outro_cancel("Call failed.")?;
						return Ok(());
					},
				}
			};
			let spinner = cliclack::spinner();
			spinner.start("Calling the contract...");

			let call_result = call_smart_contract(call_exec, weight_limit, &self.url)
				.await
				.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;

			Cli.info(call_result)?;
		}
		if self.contract.is_none() {
			let another_call: bool =
				Cli.confirm("Do you want to do another call?").initial_value(false).interact()?;
			if another_call {
				Box::pin(self.execute()).await?;
			}
		}

		Cli.outro("Call completed successfully!")?;
		Ok(())
	}
}

/// Guide the user to call the contract.
async fn guide_user_to_call_contract() -> anyhow::Result<CallContractCommand> {
	Cli.intro("Call a contract")?;

	// Prompt for location of your contract.
	let input_path: String = Cli
		.input("Where is your project located?")
		.placeholder("./")
		.default_input("./")
		.interact()?;
	let contract_path = Path::new(&input_path);

	// Prompt for contract address.
	let contract_address: String = Cli
		.input("Paste the on-chain contract address:")
		.placeholder("e.g. 5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
		.validate(|input: &String| match parse_account(input) {
			Ok(_) => Ok(()),
			Err(_) => Err("Invalid address."),
		})
		.default_input("5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
		.interact()?;

	let messages = match get_messages(contract_path) {
		Ok(messages) => messages,
		Err(e) => {
			Cli.outro_cancel("Unable to fetch contract metadata.")?;
			return Err(anyhow!(format!("{}", e.to_string())));
		},
	};
	let message = display_select_options(&messages)?;
	let mut contract_args = Vec::new();
	for arg in &message.args {
		contract_args.push(Cli.input(arg).placeholder(arg).interact()?);
	}
	let mut value = "0".to_string();
	if message.payable {
		value = Cli
			.input("Value to transfer to the call:")
			.placeholder("0")
			.default_input("0")
			.interact()?;
	}
	let mut gas_limit: Option<u64> = None;
	let mut proof_size: Option<u64> = None;
	if message.mutates {
		// Prompt for gas limit and proof_size of the call.
		let gas_limit_input: String = Cli
			.input("Enter the gas limit:")
			.required(false)
			.default_input("")
			.placeholder("If left blank, an estimation will be used")
			.interact()?;
		gas_limit = gas_limit_input.parse::<u64>().ok(); // If blank or bad input, estimate it.
		let proof_size_input: String = Cli
			.input("Enter the proof size limit:")
			.required(false)
			.placeholder("If left blank, an estimation will be used")
			.default_input("")
			.interact()?;
		proof_size = proof_size_input.parse::<u64>().ok(); // If blank or bad input, estimate it.
	}

	// Prompt for contract location.
	let url: String = Cli
		.input("Where is your contract deployed?")
		.placeholder("ws://localhost:9944")
		.default_input("ws://localhost:9944")
		.interact()?;

	// Who is calling the contract.
	let suri: String = Cli
		.input("Signer calling the contract:")
		.placeholder("//Alice")
		.default_input("//Alice")
		.interact()?;

	let mut is_call_confirmed: bool = true;
	if message.mutates {
		is_call_confirmed = Cli
			.confirm("Do you want to execute the call? (Selecting 'No' will perform a dry run)")
			.initial_value(true)
			.interact()?;
	}

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
		dry_run: !is_call_confirmed,
	})
}

fn display_select_options(messages: &Vec<Message>) -> Result<&Message> {
	let mut cli = Cli;
	let mut prompt = cli.select("Select the message to call:");
	for message in messages {
		prompt = prompt.item(message, &message.label, &message.docs);
	}
	let selected_message = prompt.interact()?;
	Ok(selected_message)
}
