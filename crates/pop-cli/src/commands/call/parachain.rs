// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{
	construct_extrinsic, encode_call_data, set_up_api, sign_and_submit_extrinsic,
	supported_extrinsics, DynamicPayload, Extrinsic, OnlineClient, SubstrateConfig,
};

#[derive(Args, Clone)]
pub struct CallParachainCommand {
	/// The name of the pallet to call.
	#[clap(long, short)]
	pallet: Option<String>,
	/// The name of the extrinsic to submit.
	#[clap(long, short)]
	extrinsic: Option<String>,
	/// The constructor arguments, encoded as strings.
	#[clap(long, num_args = 0..)]
	args: Vec<String>,
	/// Websocket endpoint of a node.
	#[clap(name = "url", long, value_parser, default_value = "ws://127.0.0.1:9944")]
	url: String,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(name = "suri", long, short, default_value = "//Alice")]
	suri: String,
}

impl CallParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		let (api, url) = self.set_up_api(&mut cli::Cli).await?;
		let mut call_config = if self.pallet.is_none() && self.extrinsic.is_none() {
			match guide_user_to_call_chain(&api, "", &url, &mut cli::Cli).await {
				Ok(call_config) => call_config,
				Err(e) => {
					display_message(&format!("{}", e), false, &mut cli::Cli)?;
					return Ok(());
				},
			}
		} else {
			self.clone()
		};
		prepare_and_submit_extrinsic(
			api,
			&mut call_config,
			self.pallet.is_none() && self.extrinsic.is_none(),
			&mut cli::Cli,
		)
		.await?;
		Ok(())
	}
	/// Prompt the user for the chain to use if not indicated and fetch the metadata.
	async fn set_up_api(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<(OnlineClient<SubstrateConfig>, String)> {
		cli.intro("Call a parachain")?;
		let url: String = if self.pallet.is_none() && self.extrinsic.is_none() {
			// Prompt for contract location.
			cli.input("Which chain would you like to interact with?")
				.placeholder("wss://rpc1.paseo.popnetwork.xyz")
				.default_input("wss://rpc1.paseo.popnetwork.xyz")
				.interact()?
		} else {
			self.url.clone()
		};
		let api = set_up_api(&url).await?;
		Ok((api, url))
	}
	/// Display the call.
	fn display(&self) -> String {
		let mut full_message = "pop call parachain".to_string();
		if let Some(pallet) = &self.pallet {
			full_message.push_str(&format!(" --pallet {}", pallet));
		}
		if let Some(extrinsic) = &self.extrinsic {
			full_message.push_str(&format!(" --extrinsic {}", extrinsic));
		}
		if !self.args.is_empty() {
			full_message.push_str(&format!(" --args {}", self.args.join(" ")));
		}
		full_message.push_str(&format!(" --url {}", self.url));
		if !self.suri.is_empty() {
			full_message.push_str(&format!(" --suri {}", self.suri));
		}
		full_message
	}
}

/// Guide the user to call the contract.
async fn guide_user_to_call_chain(
	api: &OnlineClient<SubstrateConfig>,
	suri: &str,
	url: &str,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<CallParachainCommand> {
	let extrinsic = {
		let mut prompt_extrinsic = cli.select("What would you like to do?");
		//for extrinsic in pallet.extrinsics() {
		for extrinsic in supported_extrinsics(api) {
			prompt_extrinsic = prompt_extrinsic.item(
				extrinsic.clone(),
				extrinsic.description(),
				extrinsic.pallet(),
			);
		}
		prompt_extrinsic.interact()?
	};
	let args = prompt_arguments(&extrinsic, cli)?;

	Ok(CallParachainCommand {
		pallet: Some(extrinsic.pallet().to_string()),
		extrinsic: Some(extrinsic.extrinsic_name().to_string()),
		args,
		url: url.to_string(),
		suri: suri.to_string(),
	})
}

/// Prepares the extrinsic or query.
async fn prepare_and_submit_extrinsic(
	api: OnlineClient<SubstrateConfig>,
	call_config: &mut CallParachainCommand,
	prompt_to_repeat_call: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	let extrinsic: String = call_config
		.extrinsic
		.clone()
		.expect("extrinsic can not be none as fallback above is interactive input; qed");
	let pallet = match call_config.pallet.clone() {
		Some(m) => m,
		None => {
			return Err(anyhow!("Please specify the pallet to call."));
		},
	};
	let tx = match construct_extrinsic(&pallet, &extrinsic, call_config.args.clone()) {
		Ok(tx) => tx,
		Err(e) => {
			display_message(&format!("Error parsing the arguments: {}", e), false, &mut cli::Cli)?;
			return Ok(());
		},
	};
	cli.info(format!("Encoded call data: {}", encode_call_data(&api, &tx)?))?;
	if call_config.suri.is_empty() {
		call_config.suri = cli::Cli
			.input("Who is going to sign the extrinsic:")
			.placeholder("//Alice")
			.default_input("//Alice")
			.interact()?;
	}
	cli.info(call_config.display())?;
	if !cli.confirm("Do you want to submit the call?").initial_value(true).interact()? {
		display_message(
			&format!("Extrinsic {} was not submitted. Operation canceled by the user.", extrinsic),
			false,
			cli,
		)?;
		return Ok(());
	}
	send_extrinsic(api, tx, &call_config.url, &call_config.suri, prompt_to_repeat_call, cli)
		.await?;

	Ok(())
}

async fn send_extrinsic(
	api: OnlineClient<SubstrateConfig>,
	tx: DynamicPayload,
	url: &str,
	suri: &str,
	prompt_to_repeat_call: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	let spinner = cliclack::spinner();
	spinner.start("Signing and submitting the extrinsic, please wait...");
	let result = sign_and_submit_extrinsic(api.clone(), tx, suri).await;
	if let Err(e) = result {
		console::Term::stderr().clear_last_lines(1)?;
		display_message(&format!("{}", e), false, cli)?;
	} else {
		console::Term::stderr().clear_last_lines(1)?;
		display_message(&format!("Extrinsic submitted with hash: {:?}", result?), true, cli)?;
		// Repeat call.
		if prompt_to_repeat_call {
			let another_call: bool = cli
				.confirm("Do you want to do another call to the same chain?")
				.initial_value(false)
				.interact()?;
			if another_call {
				// Remove only the prompt asking for another call.
				console::Term::stderr().clear_last_lines(2)?;
				let mut new_call_config = guide_user_to_call_chain(&api, suri, url, cli).await?;
				Box::pin(prepare_and_submit_extrinsic(
					api,
					&mut new_call_config,
					prompt_to_repeat_call,
					cli,
				))
				.await?;
			}
		}
	}
	Ok(())
}

fn display_message(message: &str, success: bool, cli: &mut impl cli::traits::Cli) -> Result<()> {
	if success {
		cli.outro(message)?;
	} else {
		cli.outro_cancel(message)?;
	}
	Ok(())
}
// Prompt the user to select an operation.
fn prompt_arguments(extrinsic: &Extrinsic, cli: &mut impl cli::traits::Cli) -> Result<Vec<String>> {
	let mut args: Vec<String> = Vec::new();
	match extrinsic {
		Extrinsic::CreateAsset => {
			args.push(prompt_for_numeric_value("Enter the Asset ID", cli)?);
			args.push(prompt_for_account("Enter the Admin Address", cli)?);
			args.push(prompt_for_numeric_value("Enter the Minimum Balance", cli)?);
		},
		Extrinsic::MintAsset => {
			args.push(prompt_for_numeric_value("Enter the Asset ID", cli)?);
			args.push(prompt_for_account("Enter the Beneficiary Address", cli)?);
			args.push(prompt_for_numeric_value("Enter the Amount", cli)?);
		},
		Extrinsic::CreateCollection => {
			args.push(prompt_for_account("Enter the Admin Address", cli)?);
			args.extend(prompt_for_collection_config(cli)?);
		},
		Extrinsic::MintNFT => {
			args.push(prompt_for_numeric_value("Enter the Collection ID", cli)?);
			args.push(prompt_for_numeric_value("Enter the Item ID", cli)?);
			args.push(prompt_for_account("Enter the Beneficiary Address", cli)?);
			args.extend(prompt_for_witness_data(cli)?);
		},
		Extrinsic::Transfer => {
			args.push(prompt_for_account("Enter the Destination Address", cli)?);
			args.push(prompt_for_numeric_value("Enter the Amount", cli)?);
		},
	}
	Ok(args)
}
fn prompt_for_numeric_value(message: &str, cli: &mut impl cli::traits::Cli) -> Result<String> {
	let id = cli
		.input(message)
		.placeholder("0")
		.default_input("0")
		.validate(|input: &String| match input.parse::<u128>() {
			Ok(_) => Ok(()),
			Err(_) => Err("Invalid value."),
		})
		.required(true)
		.interact()?;
	Ok(id)
}
fn prompt_for_account(message: &str, cli: &mut impl cli::traits::Cli) -> Result<String> {
	let account: String = cli
		.input(message)
		.placeholder("e.g. 5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
		.required(true)
		.interact()?;
	Ok(account)
}
fn prompt_for_numeric_optional_value(
	message: &str,
	cli: &mut impl cli::traits::Cli,
) -> Result<String> {
	let value = cli
		.input(message)
		.placeholder("0 or (empty for None)")
		.validate(|input: &String| match input.parse::<u128>() {
			Ok(_) => Ok(()),
			Err(_) =>
				if input.is_empty() || input == "None" {
					Ok(())
				} else {
					Err("Invalid value.")
				},
		})
		.required(false)
		.interact()?;
	if value.is_empty() || value == "None" {
		Ok("None".to_string())
	} else {
		Ok(value)
	}
}
fn prompt_for_variant_value(
	message: &str,
	default_value: &str,
	cli: &mut impl cli::traits::Cli,
) -> Result<String> {
	let mint_type: String = cli
		.input(message)
		.placeholder(&format!("e.g. {}", default_value))
		.default_input(default_value)
		.required(true)
		.interact()?;
	Ok(mint_type)
}
fn prompt_for_collection_config(cli: &mut impl cli::traits::Cli) -> Result<Vec<String>> {
	let mut args: Vec<String> = Vec::new();
	cli.info("Enter the Pallet NFT Collection Config:")?;
	args.push(prompt_for_numeric_value("Collection's Settings", cli)?);
	args.push(prompt_for_numeric_optional_value("Collection's Max Supply", cli)?);
	cli.info("Enter the Mint Settings:")?;
	args.push(prompt_for_variant_value("Who can mint?", "Issuer", cli)?);
	args.push(prompt_for_numeric_optional_value("Price per mint", cli)?);
	args.push(prompt_for_numeric_optional_value("When the mint starts", cli)?);
	args.push(prompt_for_numeric_optional_value("When the mint ends", cli)?);
	args.push(prompt_for_numeric_value("Default Item Settings", cli)?);
	Ok(args)
}
fn prompt_for_witness_data(cli: &mut impl cli::traits::Cli) -> Result<Vec<String>> {
	let mut args: Vec<String> = Vec::new();
	if cli
		.confirm("Do you want to enter witness data for mint")
		.initial_value(false)
		.interact()?
	{
		args.push(prompt_for_numeric_optional_value(
			"Id of the item in a required collection:",
			cli,
		)?);
		args.push(prompt_for_numeric_optional_value("Mint price:", cli)?);
	} else {
		args.push("None".to_string());
	}
	Ok(args)
}
