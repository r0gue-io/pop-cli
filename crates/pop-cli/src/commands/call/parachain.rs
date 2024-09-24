// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::Result;
use clap::Args;
use pop_common::parse_account;
use pop_parachains::{
	prepare_extrinsic, set_up_api, submit_extrinsic, Extrinsic, OnlineClient, Pallet,
	SubstrateConfig, Value,
};
use strum::VariantArray;

#[derive(Args, Clone)]
pub struct CallParachainCommand {
	/// The signed extrinsic to submit.
	#[clap(long, short)]
	extrinsic: Option<String>,
	/// Websocket endpoint of a node.
	#[clap(name = "url", long, value_parser, default_value = "ws://localhost:9944")]
	url: String,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(name = "suri", long, short, default_value = "//Alice")]
	suri: String,
	// pallet: Option<String>,
	// ext: Option<String>,
	// args: Option<Vec<Value>>,
}

impl CallParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		let (api, url) = self.set_up_api(&mut cli::Cli).await?;
		let call_config = if self.extrinsic.is_none() {
			guide_user_to_call_chain(&api, url, &mut cli::Cli).await?
		} else {
			self.clone()
		};
		execute_extrinsic(api, call_config, self.extrinsic.is_none(), &mut cli::Cli).await?;
		Ok(())
	}
	/// Prompt the user for the chain to use if not indicated and fetch the metadata.
	async fn set_up_api(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<(OnlineClient<SubstrateConfig>, String)> {
		cli.intro("Call a parachain")?;
		let url: String = if self.extrinsic.is_none() {
			// Prompt for contract location.
			cli.input("Which chain would you like to interact with?")
				.placeholder("ws://127.0.0.1:53677")
				.default_input("ws://127.0.0.1:53677")
				.interact()?
		} else {
			self.url.clone()
		};
		let api = set_up_api(&url).await?;
		Ok((api, url))
	}

	fn display(&self) -> String {
		let mut full_message = "pop call parachain".to_string();
		if let Some(extrinsic) = &self.extrinsic {
			full_message.push_str(&format!(" --extrinsic {}", extrinsic));
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
	url: String,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<CallParachainCommand> {
	// let pallets = Pallet::VARIANTS;
	// let pallet = {
	// 	let mut prompt = cli.select("Select the pallet to call:");
	// 	for pallet_item in pallets {
	// 		prompt = prompt.item(pallet_item.clone(), pallet_item.as_ref(), "");
	// 	}
	// 	prompt.interact()?
	// };

	let extrinsic = {
		let mut prompt_extrinsic = cli.select("Select the extrinsic to call:");
		//for extrinsic in pallet.extrinsics() {
		for extrinsic in Extrinsic::VARIANTS {
			prompt_extrinsic = prompt_extrinsic.item(
				extrinsic.clone(),
				extrinsic.description(),
				extrinsic.pallet(),
			);
		}
		prompt_extrinsic.interact()?
	};
	let args = prompt_arguments(&extrinsic, cli)?;
	let suri = cli::Cli
		.input("Who is going to sign the extrinsic:")
		.placeholder("//Alice")
		.default_input("//Alice")
		.interact()?;
	// TODO: Handle error
	let encoded_call_data =
		prepare_extrinsic(api, extrinsic.pallet(), extrinsic.extrinsic_name(), args, &suri).await?;
	Ok(CallParachainCommand { extrinsic: Some(encoded_call_data), url, suri })
}

/// Executes the extrinsic or query.
async fn execute_extrinsic(
	api: OnlineClient<SubstrateConfig>,
	call_config: CallParachainCommand,
	prompt_to_repeat_call: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	cli.info(call_config.display())?;
	let extrinsic = call_config
		.extrinsic
		.expect("extrinsic can not be none as fallback above is interactive input; qed");
	if !cli.confirm("Do you want to submit the call?").interact()? {
		display_message(&format!("Extrinsic: {} not submitted", extrinsic), true, cli)?;
		return Ok(());
	}
	let spinner = cliclack::spinner();
	spinner.start("Submitting the extrinsic...");
	// TODO: Handle error
	match submit_extrinsic(api.clone(), extrinsic).await {
		Ok(result) => {
			display_message(
				&format!("Extrinsic submitted successfully with hash: {:?}", result),
				true,
				cli,
			)?;
		},
		Err(e) => {
			display_message(&format!("Error submitting extrinsic: {}", e), false, cli)?;
		},
	}
	spinner.stop("message");
	// Repeat call.
	if prompt_to_repeat_call {
		let another_call: bool = cli
			.confirm("Do you want to do another call to the same chain?")
			.initial_value(false)
			.interact()?;
		if another_call {
			// Remove only the prompt asking for another call.
			console::Term::stderr().clear_last_lines(2)?;
			let new_call_config = guide_user_to_call_chain(&api, call_config.url, cli).await?;
			Box::pin(execute_extrinsic(api, new_call_config, prompt_to_repeat_call, cli)).await?;
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
fn prompt_arguments(extrinsic: &Extrinsic, cli: &mut impl cli::traits::Cli) -> Result<Vec<Value>> {
	let mut args: Vec<Value> = Vec::new();
	match extrinsic {
		Extrinsic::CreateAsset => {
			args.push(prompt_for_numeric_value("Enter the Asset ID", cli)?);
			args.push(prompt_for_account("Enter the Admin Address", cli)?);
			args.push(prompt_for_numeric_value("Enter the Minimum Balance", cli)?);
			// TODO: ADD METEDATA
		},
		Extrinsic::MintAsset => {
			args.push(prompt_for_numeric_value("Enter the Asset ID", cli)?);
			args.push(prompt_for_account("Enter the Beneficiary Address", cli)?);
			args.push(prompt_for_numeric_value("Enter the Amount", cli)?);
		},
		Extrinsic::CreateCollection => {
			args.push(prompt_for_account("Enter the Admin Address", cli)?);
			args.push(prompt_for_collection_config(cli)?);
		},
		Extrinsic::MintNFT => {
			args.push(prompt_for_numeric_value("Enter the Collection ID", cli)?);
			args.push(prompt_for_numeric_value("Enter the Item ID", cli)?);
			args.push(prompt_for_account("Enter the Beneficiary Address", cli)?);
			args.push(prompt_for_witness_data(cli)?);
		},
		Extrinsic::Transfer => {
			args.push(prompt_for_account("Enter the Destination Address", cli)?);
			args.push(prompt_for_numeric_value("Enter the Amount", cli)?);
		},
	}
	Ok(args)
}
fn prompt_for_numeric_value(message: &str, cli: &mut impl cli::traits::Cli) -> Result<Value> {
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
	Ok(Value::u128(id.parse::<u128>()?))
}
fn prompt_for_account(message: &str, cli: &mut impl cli::traits::Cli) -> Result<Value> {
	let account: String = cli
		.input(message)
		.placeholder("e.g. 5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
		.required(true)
		.interact()?;
	let account_id = parse_account(&account)?;
	// TODO: Support other Adresses? Let the user pick Id, Address, or Index
	Ok(Value::unnamed_variant("Id", vec![Value::from_bytes(account_id)]))
}
fn prompt_for_numeric_optional_value(
	message: &str,
	cli: &mut impl cli::traits::Cli,
) -> Result<Value> {
	let value = cli
		.input(message)
		.placeholder("0 or (empty for None)")
		.validate(|input: &String| match input.parse::<u128>() {
			Ok(_) => Ok(()),
			Err(_) => {
				if input.is_empty() || input == "None" {
					Ok(())
				} else {
					Err("Invalid value.")
				}
			},
		})
		.required(false)
		.interact()?;
	if value.is_empty() || value == "None" {
		Ok(Value::unnamed_variant("None", vec![]))
	} else {
		Ok(Value::unnamed_variant("Some", vec![Value::u128(value.parse::<u128>()?)]))
	}
}
fn prompt_for_variant_value(
	message: &str,
	default_value: &str,
	cli: &mut impl cli::traits::Cli,
) -> Result<Value> {
	let mint_type: String = cli
		.input(message)
		.placeholder(&format!("e.g. {}", default_value))
		.default_input(default_value)
		.required(true)
		.interact()?;
	Ok(Value::unnamed_variant(mint_type, vec![]))
}
fn prompt_for_collection_config(cli: &mut impl cli::traits::Cli) -> Result<Value> {
	cli.info("Enter the Pallet NFT Collection Config:")?;
	let settings = prompt_for_numeric_value("Collection's Settings", cli)?;
	let max_supply = prompt_for_numeric_optional_value("Collection's Max Supply", cli)?;
	cli.info("Enter the Mint Settings:")?;
	let mint_type = prompt_for_variant_value("Who can mint?", "Issuer", cli)?;
	let price_per_mint = prompt_for_numeric_optional_value("Price per mint", cli)?;
	let start_block = prompt_for_numeric_optional_value("When the mint starts", cli)?;
	let end_block = prompt_for_numeric_optional_value("When the mint ends", cli)?;
	let default_item_settings = prompt_for_numeric_value("Default Item Settings", cli)?;
	// mint settings
	let mint_settings = Value::unnamed_composite(vec![
		mint_type,
		price_per_mint,
		start_block,
		end_block,
		default_item_settings,
	]);
	let config_collection = Value::unnamed_composite(vec![settings, max_supply, mint_settings]);

	Ok(config_collection)
}
fn prompt_for_witness_data(cli: &mut impl cli::traits::Cli) -> Result<Value> {
	if cli
		.confirm("Do you want to enter witness data for mint")
		.initial_value(false)
		.interact()?
	{
		let owned_item =
			prompt_for_numeric_optional_value("Id of the item in a required collection:", cli)?;
		let mint_price = prompt_for_numeric_optional_value("Mint price:", cli)?;
		Ok(Value::unnamed_variant("Some".to_string(), vec![owned_item, mint_price]))
	} else {
		Ok(Value::unnamed_variant("None", vec![]))
	}
}
