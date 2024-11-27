// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{
	construct_extrinsic, encode_call_data, find_extrinsic_by_name, find_pallet_by_name,
	parse_chain_metadata, set_up_api, sign_and_submit_extrinsic, supported_actions, Action,
	DynamicPayload, Extrinsic, OnlineClient, Pallet, Param, SubstrateConfig,
};
use url::Url;

const DEFAULT_URL: &str = "ws://localhost:9944/";
const DEFAULT_URI: &str = "//Alice";

#[derive(Args, Clone)]
pub struct CallParachainCommand {
	/// The pallet containing the extrinsic to execute.
	#[arg(long)]
	pallet: Option<String>,
	/// The extrinsic to execute within the chosen pallet.
	#[arg(long)]
	extrinsic: Option<String>,
	/// The extrinsic arguments, encoded as strings.
	#[arg(long, num_args = 0..,)]
	args: Vec<String>,
	/// Websocket endpoint of a node.
	#[arg(short = 'u', long, value_parser)]
	url: Option<Url>,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[arg(long)]
	suri: Option<String>,
	/// Automatically signs and submits the extrinsic without asking for confirmation.
	#[arg(short('y'), long)]
	skip_confirm: bool,
}

impl CallParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		let mut cli = cli::Cli;
		cli.intro("Call a parachain")?;

		// Configure the chain.
		let chain = self.configure_chain(&mut cli).await?;
		loop {
			// Configure the call based on command line arguments/call UI.
			let mut call = match self.configure_call(&chain, &mut cli).await {
				Ok(call) => call,
				Err(e) => {
					display_message(&e.to_string(), false, &mut cli)?;
					break
				},
			};
			// Display the configured call.
			cli.info(call.display(&chain))?;
			// Prepare the extrinsic.
			let tx = match call.prepare_extrinsic(&chain.api, &mut cli).await {
				Ok(api) => api,
				Err(e) => {
					display_message(&e.to_string(), false, &mut cli)?;
					break
				},
			};
			// TODO: If call_data, go directly here (?).
			// Send the extrinsic.
			if let Err(e) = call.send_extrinsic(&chain.api, tx, &mut cli).await {
				display_message(&e.to_string(), false, &mut cli)?;
			}
			if !cli
				.confirm("Do you want to perform another call?")
				.initial_value(false)
				.interact()?
			{
				display_message("Parachain calling complete.", true, &mut cli)?;
				break
			}
		}
		Ok(())
	}

	async fn configure_chain(&self, cli: &mut impl Cli) -> Result<Chain> {
		// Resolve url.
		let url = match self.clone().url {
			Some(url) => url,
			None => {
				// Prompt for url.
				let url: String = cli
					.input("Which chain would you like to interact with?")
					.default_input(DEFAULT_URL)
					.interact()?;
				Url::parse(&url)?
			},
		};

		// Parse metadata from chain url.
		let api = set_up_api(&url.as_str()).await?;
		let pallets = parse_chain_metadata(&api).await.map_err(|e| {
			anyhow!(format!("Unable to fetch the chain metadata: {}", e.to_string()))
		})?;
		Ok(Chain { url, api, pallets })
	}

	/// Configure the call based on command line arguments/call UI.
	async fn configure_call(&mut self, chain: &Chain, cli: &mut impl Cli) -> Result<CallParachain> {
		loop {
			// Resolve pallet.
			let pallet = match self.pallet {
				Some(ref pallet_name) => find_pallet_by_name(&chain.pallets, pallet_name).await?,
				None => {
					// Specific predefined actions first.
					if let Some(action) = prompt_predefined_actions(&chain.pallets, cli).await? {
						self.extrinsic = Some(action.extrinsic_name().to_string());
						find_pallet_by_name(&chain.pallets, action.pallet_name()).await?
					} else {
						let mut prompt = cli.select("Select the pallet to call:");
						for pallet_item in &chain.pallets {
							prompt = prompt.item(
								pallet_item.clone(),
								&pallet_item.name,
								&pallet_item.docs,
							);
						}
						prompt.interact()?
					}
				},
			};

			// Resolve extrinsic.
			let extrinsic = match self.extrinsic {
				Some(ref extrinsic_name) =>
					find_extrinsic_by_name(&chain.pallets, &pallet.name, extrinsic_name).await?,
				None => {
					let mut prompt_extrinsic = cli.select("Select the extrinsic to call:");
					for extrinsic in &pallet.extrinsics {
						prompt_extrinsic = prompt_extrinsic.item(
							extrinsic.clone(),
							&extrinsic.name,
							&extrinsic.docs,
						);
					}
					prompt_extrinsic.interact()?
				},
			};
			// Certain extrinsics are not supported yet due to complexity.
			if !extrinsic.is_supported {
				cli.outro_cancel(
					"The selected extrinsic is not supported yet. Please choose another one.",
				)?;
				continue
			}

			// Resolve message arguments.
			let args = if self.clone().args.is_empty() {
				let mut args = Vec::new();
				for param in &extrinsic.params {
					let input = prompt_for_param(&chain.api, cli, &param)?;
					args.push(input);
				}
				args
			} else {
				self.clone().args
			};

			// Resolve who is signing the extrinsic.
			let suri = match self.clone().suri {
				Some(suri) => suri,
				None =>
					cli.input("Signer of the extrinsic:").default_input(DEFAULT_URI).interact()?,
			};

			return Ok(CallParachain {
				pallet,
				extrinsic,
				args,
				suri,
				skip_confirm: self.skip_confirm,
			});
		}
	}
}

struct Chain {
	url: Url,
	api: OnlineClient<SubstrateConfig>,
	pallets: Vec<Pallet>,
}

#[derive(Clone)]
struct CallParachain {
	/// The pallet of the extrinsic.
	pallet: Pallet,
	/// The extrinsic to execute.
	extrinsic: Extrinsic,
	/// The extrinsic arguments, encoded as strings.
	args: Vec<String>,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	suri: String,
	/// Whether to automatically sign and submit the extrinsic without asking for confirmation.
	skip_confirm: bool,
}

impl CallParachain {
	// Prepares the extrinsic or query.
	async fn prepare_extrinsic(
		&self,
		api: &OnlineClient<SubstrateConfig>,
		cli: &mut impl Cli,
	) -> Result<DynamicPayload> {
		let tx = match construct_extrinsic(
			&self.pallet.name.as_str(),
			&self.extrinsic.name.as_str(),
			self.args.clone(),
		)
		.await
		{
			Ok(tx) => tx,
			Err(e) => {
				return Err(anyhow!("Error: {}", e));
			},
		};
		cli.info(format!("Encoded call data: {}", encode_call_data(api, &tx)?))?;
		Ok(tx)
	}

	// Sign and submit an extrinsic.
	async fn send_extrinsic(
		&mut self,
		api: &OnlineClient<SubstrateConfig>,
		tx: DynamicPayload,
		cli: &mut impl Cli,
	) -> Result<()> {
		if !self.skip_confirm &&
			!cli.confirm("Do you want to submit the extrinsic?")
				.initial_value(true)
				.interact()?
		{
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
			.map_err(|err| anyhow!("{}", format!("{err:?}")))?;

		spinner.stop(&format!("Extrinsic submitted with hash: {:?}", result));
		Ok(())
	}
	fn display(&self, chain: &Chain) -> String {
		let mut full_message = "pop call parachain".to_string();
		full_message.push_str(&format!(" --pallet {}", self.pallet));
		full_message.push_str(&format!(" --extrinsic {}", self.extrinsic));
		if !self.args.is_empty() {
			let args: Vec<_> = self.args.iter().map(|a| format!("\"{a}\"")).collect();
			full_message.push_str(&format!(" --args {}", args.join(" ")));
		}
		full_message.push_str(&format!(" --url {} --suri {}", chain.url, self.suri));
		full_message
	}
}

fn display_message(message: &str, success: bool, cli: &mut impl Cli) -> Result<()> {
	if success {
		cli.outro(message)?;
	} else {
		cli.outro_cancel(message)?;
	}
	Ok(())
}

// Prompts the user for some predefined actions.
async fn prompt_predefined_actions(
	pallets: &[Pallet],
	cli: &mut impl Cli,
) -> Result<Option<Action>> {
	let mut predefined_action = cli.select("What would you like to do?");
	for action in supported_actions(&pallets).await {
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
	cli: &mut impl Cli,
	param: &Param,
) -> Result<String> {
	if param.is_optional {
		if !cli
			.confirm(format!(
				"Do you want to provide a value for the optional parameter: {}?",
				param.name
			))
			.interact()?
		{
			return Ok("None()".to_string());
		}
		let value = get_param_value(api, cli, param)?;
		Ok(format!("Some({})", value))
	} else {
		get_param_value(api, cli, param)
	}
}

// Resolves the value of a parameter based on its type.
fn get_param_value(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl Cli,
	param: &Param,
) -> Result<String> {
	if param.sub_params.is_empty() {
		prompt_for_primitive_param(cli, param)
	} else if param.is_variant {
		prompt_for_variant_param(api, cli, param)
	} else if param.is_tuple {
		prompt_for_tuple_param(api, cli, param)
	} else {
		prompt_for_composite_param(api, cli, param)
	}
}

// Prompt for the value when is a primitive.
fn prompt_for_primitive_param(cli: &mut impl Cli, param: &Param) -> Result<String> {
	Ok(cli
		.input(format!("Enter the value for the parameter: {}", param.name))
		.placeholder(&format!("Type required: {}", param.type_name))
		.interact()?)
}

// Prompt the user to select the value of the Variant parameter and recursively prompt for nested
// fields. Output example: Id(5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY) for the Id variant.
fn prompt_for_variant_param(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl Cli,
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
		Ok(format!("{}()", selected_variant.name.clone()))
	}
}

// Recursively prompt the user for all the nested fields in a Composite type.
fn prompt_for_composite_param(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl Cli,
	param: &Param,
) -> Result<String> {
	let mut field_values = Vec::new();
	for field_arg in &param.sub_params {
		let field_value = prompt_for_param(api, cli, field_arg)?;
		// Example: Param { name: "Id", type_name: "AccountId32 ([u8;32])", is_optional: false,
		// sub_params: [Param { name: "Id", type_name: "[u8;32]", is_optional: false, sub_params:
		// [], is_variant: false }], is_variant: false }
		if param.sub_params.len() == 1 && param.name == param.sub_params[0].name {
			field_values.push(format!("{}", field_value));
		} else {
			field_values.push(format!("{}: {}", field_arg.name, field_value));
		}
	}
	if param.sub_params.len() == 1 && param.name == param.sub_params[0].name {
		Ok(format!("{}", field_values.join(", ")))
	} else {
		Ok(format!("{{{}}}", field_values.join(", ")))
	}
}

// Recursively prompt the user for the tuple values.
fn prompt_for_tuple_param(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl Cli,
	param: &Param,
) -> Result<String> {
	let mut tuple_values = Vec::new();
	for (_index, tuple_param) in param.sub_params.iter().enumerate() {
		let tuple_value = prompt_for_param(api, cli, tuple_param)?;
		tuple_values.push(tuple_value);
	}
	Ok(format!("({})", tuple_values.join(", ")))
}
