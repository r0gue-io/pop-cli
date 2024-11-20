// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{
	construct_extrinsic, encode_call_data, field_to_param, find_extrinsic_by_name,
	find_pallet_by_name, parse_chain_metadata, set_up_api, sign_and_submit_extrinsic,
	supported_actions, Action, DynamicPayload, OnlineClient, Param, SubstrateConfig,
};

const DEFAULT_URL: &str = "ws://localhost:9944/";
const DEFAULT_URI: &str = "//Alice";

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
	#[clap(name = "url", short = 'u', long, value_parser, default_value = DEFAULT_URL)]
	url: url::Url,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[clap(name = "suri", long, short, default_value = DEFAULT_URI)]
	suri: String,
}

impl CallParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		// Check if message specified via command line argument.
		let prompt_to_repeat_call = self.extrinsic.is_none();
		// Configure the call based on command line arguments/call UI.
		let api = match self.configure(&mut cli::Cli, false).await {
			Ok(api) => api,
			Err(e) => {
				display_message(&e.to_string(), false, &mut cli::Cli)?;
				return Ok(());
			},
		};
		// Prepare Extrinsic.
		let tx = match self.prepare_extrinsic(&api, &mut cli::Cli).await {
			Ok(api) => api,
			Err(e) => {
				display_message(&e.to_string(), false, &mut cli::Cli)?;
				return Ok(());
			},
		};
		// TODO: If call_data, go directly here.
		// Finally execute the call.
		if let Err(e) = self.send_extrinsic(api, tx, prompt_to_repeat_call, &mut cli::Cli).await {
			display_message(&e.to_string(), false, &mut cli::Cli)?;
		}
		Ok(())
	}

	/// Configure the call based on command line arguments/call UI.
	async fn configure(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		repeat: bool,
	) -> Result<OnlineClient<SubstrateConfig>> {
		// Show intro on first run.
		if !repeat {
			cli.intro("Call a parachain")?;
		}
		// If extrinsic has been specified via command line arguments, return early.
		// TODO: CALL DATA
		// if self.extrinsic.is_some() {
		// 	return Ok(());
		// }

		// Resolve url.
		if !repeat && self.url.as_str() == DEFAULT_URL {
			// Prompt for url.
			let url: String = cli
				.input("Which chain would you like to interact with?")
				.placeholder("wss://rpc1.paseo.popnetwork.xyz")
				.default_input("wss://rpc1.paseo.popnetwork.xyz")
				.interact()?;
			self.url = url::Url::parse(&url)?
		};
		// Parse metadata from url chain.
		let api = set_up_api(self.url.as_str()).await?;
		let pallets = match parse_chain_metadata(&api).await {
			Ok(pallets) => pallets,
			Err(e) => {
				return Err(anyhow!(format!(
					"Unable to fetch the chain metadata: {}",
					e.to_string()
				)));
			},
		};
		// Resolve pallet.
		let pallet = if let Some(ref pallet_name) = self.pallet {
			find_pallet_by_name(&api, pallet_name).await?
		} else {
			// Specific predefined actions first.
			let picked_action: Option<Action> = prompt_predefined_actions(&api, cli).await?;
			if let Some(action) = picked_action {
				self.extrinsic = Some(action.extrinsic_name().to_string());
				find_pallet_by_name(&api, action.pallet_name()).await?
			} else {
				let mut prompt = cli.select("Select the pallet to call:");
				for pallet_item in pallets {
					prompt = prompt.item(pallet_item.clone(), &pallet_item.name, &pallet_item.docs);
				}
				let pallet_prompted = prompt.interact()?;
				self.pallet = Some(pallet_prompted.name.clone());
				pallet_prompted
			}
		};
		// Resolve extrinsic.
		let extrinsic = if let Some(ref extrinsic_name) = self.extrinsic {
			find_extrinsic_by_name(&api, &pallet.name, extrinsic_name).await?
		} else {
			let mut prompt_extrinsic = cli.select("Select the extrinsic to call:");
			for extrinsic in pallet.extrinsics {
				prompt_extrinsic = prompt_extrinsic.item(
					extrinsic.clone(),
					&extrinsic.name,
					&extrinsic.docs.concat(),
				);
			}
			let extrinsic_prompted = prompt_extrinsic.interact()?;
			self.extrinsic = Some(extrinsic_prompted.name.clone());
			extrinsic_prompted
		};
		// Resolve message arguments.
		let mut contract_args = Vec::new();
		for field in extrinsic.fields {
			let param = field_to_param(&api, &field)?;
			let input = prompt_for_param(&api, cli, &param)?;
			contract_args.push(input);
		}
		self.args = contract_args;

		cli.info(self.display())?;
		Ok(api)
	}

	fn display(&self) -> String {
		let mut full_message = "pop call parachain".to_string();
		if let Some(pallet) = &self.pallet {
			full_message.push_str(&format!(" --pallet {}", pallet));
		}
		if let Some(extrinsic) = &self.extrinsic {
			full_message.push_str(&format!(" --extrinsic {}", extrinsic));
		}
		if !self.args.is_empty() {
			let args: Vec<_> = self.args.iter().map(|a| format!("\"{a}\"")).collect();
			full_message.push_str(&format!(" --args {}", args.join(", ")));
		}
		full_message.push_str(&format!(" --url {} --suri {}", self.url, self.suri));
		full_message
	}

	/// Prepares the extrinsic or query.
	async fn prepare_extrinsic(
		&self,
		api: &OnlineClient<SubstrateConfig>,
		cli: &mut impl cli::traits::Cli,
	) -> Result<DynamicPayload> {
		let extrinsic = match &self.extrinsic {
			Some(extrinsic) => extrinsic.to_string(),
			None => {
				return Err(anyhow!("Please specify the extrinsic."));
			},
		};
		let pallet = match &self.pallet {
			Some(pallet) => pallet.to_string(),
			None => {
				return Err(anyhow!("Please specify the pallet."));
			},
		};
		let tx = match construct_extrinsic(api, &pallet, &extrinsic, self.args.clone()).await {
			Ok(tx) => tx,
			Err(e) => {
				return Err(anyhow!("Error parsing the arguments: {}", e));
			},
		};
		cli.info(format!("Encoded call data: {}", encode_call_data(api, &tx)?))?;
		Ok(tx)
	}

	async fn send_extrinsic(
		&mut self,
		api: OnlineClient<SubstrateConfig>,
		tx: DynamicPayload,
		prompt_to_repeat_call: bool,
		cli: &mut impl cli::traits::Cli,
	) -> Result<()> {
		if self.suri.is_empty() {
			self.suri = cli::Cli
				.input("Who is going to sign the extrinsic:")
				.placeholder("//Alice")
				.default_input("//Alice")
				.interact()?;
		}
		cli.info(self.display())?;
		if !cli.confirm("Do you want to submit the call?").initial_value(true).interact()? {
			display_message(
				&format!(
					"Extrinsic {:?} was not submitted. Operation canceled by the user.",
					self.extrinsic
				),
				false,
				cli,
			)?;
			return Ok(());
		}
		let spinner = cliclack::spinner();
		spinner.start("Signing and submitting the extrinsic, please wait...");
		let result = sign_and_submit_extrinsic(api.clone(), tx, &self.suri)
			.await
			.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;

		display_message(&format!("Extrinsic submitted with hash: {:?}", result), true, cli)?;

		// Prompt for any additional calls.
		if !prompt_to_repeat_call {
			display_message("Call completed successfully!", true, cli)?;
			return Ok(());
		}
		if cli
			.confirm("Do you want to perform another call to the same chain?")
			.initial_value(false)
			.interact()?
		{
			// Reset specific items from the last call and repeat.
			self.reset_for_new_call();
			self.configure(cli, true).await?;
			let tx = self.prepare_extrinsic(&api, &mut cli::Cli).await?;
			Box::pin(self.send_extrinsic(api, tx, prompt_to_repeat_call, cli)).await
		} else {
			display_message("Parachain calling complete.", true, cli)?;
			Ok(())
		}
	}
	/// Resets specific fields to default values for a new call.
	fn reset_for_new_call(&mut self) {
		self.pallet = None;
		self.extrinsic = None;
	}
}

fn display_message(message: &str, success: bool, cli: &mut impl cli::traits::Cli) -> Result<()> {
	if success {
		cli.outro(message)?;
	} else {
		cli.outro_cancel(message)?;
	}
	Ok(())
}

async fn prompt_predefined_actions(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl cli::traits::Cli,
) -> Result<Option<Action>> {
	let mut predefined_action = cli.select("What would you like to do?");
	for action in supported_actions(&api).await {
		predefined_action = predefined_action.item(
			Some(action.clone()),
			action.description(),
			action.pallet_name(),
		);
	}
	predefined_action = predefined_action.item(None, "All", "Explore all pallets and extrinsics");
	Ok(predefined_action.interact()?)
}

// Prompts the user for the value of a parameter.
fn prompt_for_param(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl cli::traits::Cli,
	param: &Param,
) -> Result<String> {
	if param.is_optional {
		// Prompt user for optional parameter decision.
		if !cli
			.confirm(format!(
				"Do you want to provide a value for the optional parameter: {}?",
				param.name
			))
			.interact()?
		{
			return Ok("None".to_string());
		}
		let value = get_param_value(api, cli, param)?;
		Ok(format!("Some({})", value))
	} else {
		// Handle non-optional parameters.
		get_param_value(api, cli, param)
	}
}

// Resolves the value of a parameter based on its type.
fn get_param_value(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl cli::traits::Cli,
	param: &Param,
) -> Result<String> {
	if param.sub_params.is_empty() {
		prompt_for_primitive_param(cli, param)
	} else if param.is_variant {
		prompt_for_variant_param(api, cli, param)
	} else {
		prompt_for_composite_param(api, cli, param)
	}
}

fn prompt_for_primitive_param(cli: &mut impl cli::traits::Cli, param: &Param) -> Result<String> {
	Ok(cli
		.input(format!("Enter the value for the parameter: {}", param.name))
		.placeholder(&format!("Type required: {}", param.type_name))
		.interact()?)
}

fn prompt_for_variant_param(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl cli::traits::Cli,
	param: &Param,
) -> Result<String> {
	let selected_variant = {
		let mut select = cli.select(format!("Select the value for the parameter: {}", param.name));
		for option in &param.sub_params {
			select = select.item(option, &option.name, &option.type_name);
		}
		select.interact()?
	};

	if !selected_variant.sub_params.is_empty() {
		let mut field_values = Vec::new();
		for field_arg in &selected_variant.sub_params {
			let field_value = prompt_for_param(api, cli, field_arg)?;
			field_values.push(field_value);
		}
		Ok(format!("{}({})", selected_variant.name, field_values.join(", ")))
	} else {
		Ok(selected_variant.name.clone())
	}
}

fn prompt_for_composite_param(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl cli::traits::Cli,
	param: &Param,
) -> Result<String> {
	let mut field_values = Vec::new();
	for field_arg in &param.sub_params {
		let field_value = prompt_for_param(api, cli, field_arg)?;
		field_values.push(field_value);
	}
	Ok(field_values.join(", "))
}
