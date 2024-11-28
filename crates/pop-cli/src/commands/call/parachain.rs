// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_parachains::{
	construct_extrinsic, encode_call_data, find_extrinsic_by_name, find_pallet_by_name,
	parse_chain_metadata, set_up_api, sign_and_submit_extrinsic,
	sign_and_submit_extrinsic_with_call_data, supported_actions, Action, DynamicPayload,
	OnlineClient, Pallet, Param, SubstrateConfig,
};

const DEFAULT_URL: &str = "ws://localhost:9944/";
const DEFAULT_URI: &str = "//Alice";

#[derive(Args, Clone)]
pub struct CallParachainCommand {
	/// The pallet containing the extrinsic to execute.
	#[clap(long, short)]
	pallet: Option<String>,
	/// The extrinsic to execute within the chosen pallet.
	#[clap(long, short)]
	extrinsic: Option<String>,
	/// The extrinsic arguments, encoded as strings.
	#[clap(long, num_args = 0..,)]
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
	/// Automatically signs and submits the extrinsic without asking for confirmation.
	#[clap(short('y'), long)]
	skip_confirm: bool,
	/// SCALE encoded bytes representing the call data of the transaction.
	#[arg(name = "call", short = 'c', long, conflicts_with_all = ["pallet", "extrinsic", "args"])]
	call_data: Option<String>,
}

impl CallParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		let api = match self.initialize_api_client(&mut cli::Cli).await {
			Ok(api) => api,
			Err(e) => {
				display_message(&e.to_string(), false, &mut cli::Cli)?;
				return Ok(());
			},
		};
		if let Some(call_data) = self.call_data.as_ref().cloned() {
			// Execute the call.
			if let Err(e) =
				self.send_extrinsic_from_call_data(&api, &call_data, &mut cli::Cli).await
			{
				display_message(&e.to_string(), false, &mut cli::Cli)?;
			}
		} else {
			// Check if message specified via command line argument.
			let prompt_to_repeat_call = self.extrinsic.is_none();
			// Configure the call based on command line arguments/call UI.
			if let Err(e) = self.configure(&api, &mut cli::Cli).await {
				display_message(&e.to_string(), false, &mut cli::Cli)?;
				return Ok(());
			}
			// Prepare Extrinsic.
			let tx = match self.prepare_extrinsic(&api, &mut cli::Cli).await {
				Ok(api) => api,
				Err(e) => {
					display_message(&e.to_string(), false, &mut cli::Cli)?;
					return Ok(());
				},
			};
			// Execute the call.
			if let Err(e) = self.send_extrinsic(api, tx, prompt_to_repeat_call, &mut cli::Cli).await
			{
				display_message(&e.to_string(), false, &mut cli::Cli)?;
			}
		}
		Ok(())
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
			full_message.push_str(&format!(" --args {}", args.join(" ")));
		}
		full_message.push_str(&format!(" --url {} --suri {}", self.url, self.suri));
		full_message
	}

	/// Initializes the API client by configuring the chain URL.
	async fn initialize_api_client(
		&mut self,
		cli: &mut impl cli::traits::Cli,
	) -> Result<OnlineClient<SubstrateConfig>> {
		// Show intro on first run.
		cli.intro("Call a parachain")?;

		// If extrinsic has been specified via command line arguments, return early.
		if self.extrinsic.is_some() {
			return Ok(set_up_api(self.url.as_str()).await?);
		}

		// Resolve url.
		if self.url.as_str() == DEFAULT_URL {
			// Prompt for url.
			let url: String = cli
				.input("Which chain would you like to interact with?")
				.placeholder("wss://rpc1.paseo.popnetwork.xyz")
				.default_input("wss://rpc1.paseo.popnetwork.xyz")
				.interact()?;
			self.url = url::Url::parse(&url)?
		};
		let api = set_up_api(self.url.as_str()).await?;
		Ok(api)
	}

	/// Configure the call based on command line arguments/call UI.
	async fn configure(
		&mut self,
		api: &OnlineClient<SubstrateConfig>,
		cli: &mut impl cli::traits::Cli,
	) -> Result<()> {
		let pallets = match parse_chain_metadata(api).await {
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
			// Specified by the user.
			find_pallet_by_name(&pallets, pallet_name).await?
		} else {
			// Specific predefined actions first.
			let action: Option<Action> = prompt_predefined_actions(&pallets, cli).await?;
			if let Some(action) = action {
				self.pallet = Some(action.pallet_name().to_string());
				self.extrinsic = Some(action.extrinsic_name().to_string());
				find_pallet_by_name(&pallets, action.pallet_name()).await?
			} else {
				let mut prompt = cli.select("Select the pallet to call:");
				for pallet_item in &pallets {
					prompt = prompt.item(pallet_item.clone(), &pallet_item.name, &pallet_item.docs);
				}
				let pallet_prompted = prompt.interact()?;
				self.pallet = Some(pallet_prompted.name.clone());
				pallet_prompted
			}
		};
		// Resolve extrinsic.
		let extrinsic = if let Some(ref extrinsic_name) = self.extrinsic {
			find_extrinsic_by_name(&pallets, &pallet.name, extrinsic_name).await?
		} else {
			let mut prompt_extrinsic = cli.select("Select the extrinsic to call:");
			for extrinsic in pallet.extrinsics {
				prompt_extrinsic =
					prompt_extrinsic.item(extrinsic.clone(), &extrinsic.name, &extrinsic.docs);
			}
			let extrinsic_prompted = prompt_extrinsic.interact()?;
			self.extrinsic = Some(extrinsic_prompted.name.clone());
			extrinsic_prompted
		};
		// Certain extrinsics are not supported yet due to complexity.
		if !extrinsic.is_supported {
			cli.outro_cancel(
				"The selected extrinsic is not supported yet. Please choose another one.",
			)?;
			// Reset specific items from the last call and repeat.
			self.reset_for_new_call();
			Box::pin(self.configure(api, cli)).await?;
		}

		// Resolve message arguments.
		if self.args.is_empty() {
			let mut contract_args = Vec::new();
			for param in extrinsic.params {
				let input = prompt_for_param(api, cli, &param)?;
				contract_args.push(input);
			}
			self.args = contract_args;
		}

		// Resolve who is sigining the extrinsic.
		if self.suri == DEFAULT_URI {
			self.suri = cli
				.input("Signer of the extrinsic:")
				.placeholder("//Alice")
				.default_input("//Alice")
				.interact()?;
		}

		cli.info(self.display())?;
		Ok(())
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
		let tx = match construct_extrinsic(&pallet, &extrinsic, self.args.clone()).await {
			Ok(tx) => tx,
			Err(e) => {
				return Err(anyhow!("Error: {}", e));
			},
		};
		cli.info(format!("Encoded call data: {}", encode_call_data(api, &tx)?))?;
		Ok(tx)
	}

	/// Sign an submit an extrinsic.
	async fn send_extrinsic(
		&mut self,
		api: OnlineClient<SubstrateConfig>,
		tx: DynamicPayload,
		prompt_to_repeat_call: bool,
		cli: &mut impl cli::traits::Cli,
	) -> Result<()> {
		if !self.skip_confirm
			&& !cli
				.confirm("Do you want to submit the extrinsic?")
				.initial_value(true)
				.interact()?
		{
			display_message(
				&format!("Extrinsic not submitted. Operation canceled by the user."),
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

		// Prompt for any additional calls.
		if !prompt_to_repeat_call {
			display_message("Extrinsic submitted successfully!", true, cli)?;
			return Ok(());
		}
		if cli
			.confirm("Do you want to perform another call to the same chain?")
			.initial_value(false)
			.interact()?
		{
			// Reset specific items from the last call and repeat.
			self.reset_for_new_call();
			self.configure(&api, cli).await?;
			let tx = self.prepare_extrinsic(&api, &mut cli::Cli).await?;
			Box::pin(self.send_extrinsic(api, tx, prompt_to_repeat_call, cli)).await
		} else {
			display_message("Parachain calling complete.", true, cli)?;
			Ok(())
		}
	}
	// Sends an extrinsic to the chain using the call data.
	async fn send_extrinsic_from_call_data(
		&mut self,
		api: &OnlineClient<SubstrateConfig>,
		call_data: &str,
		cli: &mut impl cli::traits::Cli,
	) -> Result<()> {
		if self.suri == DEFAULT_URI {
			self.suri = cli
				.input("Signer of the extrinsic:")
				.placeholder("//Alice")
				.default_input("//Alice")
				.interact()?;
		}
		cli.info(format!("Encoded call data: {}", call_data))?;
		if !cli
			.confirm("Do you want to submit the extrinsic?")
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
		let result = sign_and_submit_extrinsic_with_call_data(api.clone(), call_data, &self.suri)
			.await
			.map_err(|err| anyhow!("{}", format!("{err:?}")))?;

		spinner.stop(&format!("Extrinsic submitted successfully with hash: {:?}", result));
		display_message("Parachain calling complete.", true, cli)?;
		Ok(())
	}
	/// Resets specific fields to default values for a new call.
	fn reset_for_new_call(&mut self) {
		self.pallet = None;
		self.extrinsic = None;
		self.args.clear();
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

// Prompts the user for some predefined actions.
async fn prompt_predefined_actions(
	pallets: &[Pallet],
	cli: &mut impl cli::traits::Cli,
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
	cli: &mut impl cli::traits::Cli,
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
	cli: &mut impl cli::traits::Cli,
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
fn prompt_for_primitive_param(cli: &mut impl cli::traits::Cli, param: &Param) -> Result<String> {
	Ok(cli
		.input(format!("Enter the value for the parameter: {}", param.name))
		.placeholder(&format!("Type required: {}", param.type_name))
		.interact()?)
}

// Prompt the user to select the value of the Variant parameter and recursively prompt for nested
// fields. Output example: Id(5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY) for the Id variant.
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
		Ok(format!("{}()", selected_variant.name.clone()))
	}
}

// Recursively prompt the user for all the nested fields in a Composite type.
fn prompt_for_composite_param(
	api: &OnlineClient<SubstrateConfig>,
	cli: &mut impl cli::traits::Cli,
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
	cli: &mut impl cli::traits::Cli,
	param: &Param,
) -> Result<String> {
	let mut tuple_values = Vec::new();
	for (_index, tuple_param) in param.sub_params.iter().enumerate() {
		let tuple_value = prompt_for_param(api, cli, tuple_param)?;
		tuple_values.push(tuple_value);
	}
	Ok(format!("({})", tuple_values.join(", ")))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use url::Url;

	#[tokio::test]
	async fn initialize_api_client_works() -> Result<()> {
		let mut cli = MockCli::new().expect_intro("Call a parachain");
		let mut call_config = CallParachainCommand {
			pallet: None,
			extrinsic: Some("Test".to_string()),
			args: vec![].to_vec(),
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: DEFAULT_URI.to_string(),
			skip_confirm: false,
			call_data: None,
		};
		// Test exit early extrinsic specified via command line arguments.
		call_config.initialize_api_client(&mut cli).await?;
		cli.verify()?;
		// Test prompt for the chain
		call_config.extrinsic = None;
		call_config.url = Url::parse(DEFAULT)?;
		cli = MockCli::new().expect_intro("Call a parachain").expect_input(
			"Which chain would you like to interact with?",
			"wss://rpc1.paseo.popnetwork.xyz".into(),
		);
		call_config.initialize_api_client(&mut cli).await?;

		cli.verify()
	}
	
	// This test only covers the interactive portion of the call parachain command, without actually
	// submitting any extrinsic.
	#[tokio::test]
	async fn guide_user_to_call_parachain_works() -> Result<()> {
		let mut cli = MockCli::new()
		.expect_input("Signer of the extrinsic:", "//Bob".into())
		.expect_input("Enter the value for the parameter: remark", "0x11".into())
		.expect_select::<Pallet>(
			"Select the extrinsic to call:",
			Some(true),
			true,
			Some(
				[
					("remark".to_string(), "Make some on-chain remark.Can be executed by every `origin`.".to_string()),
					("set_heap_pages".to_string(), "Set the number of pages in the WebAssembly environment's heap.".to_string()),
					("set_code".to_string(), "Set the new runtime code.".to_string()),
					("set_code_without_checks".to_string(), "Set the new runtime code without doing any checks of the given `code`.Note that runtime upgrades will not run if this is called with a not-increasing specversion!".to_string()),
					("set_storage".to_string(), "Set some items of storage.".to_string()),
					("kill_storage".to_string(), "Kill some items from storage.".to_string()),
					("kill_prefix".to_string(), "Kill all storage items with a key that starts with the given prefix.**NOTE:** We rely on the Root origin to provide us the number of subkeys underthe prefix we are removing to accurately calculate the weight of this function.".to_string()),
					("remark_with_event".to_string(), "Make some on-chain remark and emit event.".to_string()),
					("authorize_upgrade".to_string(), "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be suppliedlater.This call requires Root origin.".to_string()),
					("authorize_upgrade_without_checks".to_string(), "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be suppliedlater.WARNING: This authorizes an upgrade that will take place without any safety checks, forexample that the spec name remains the same and that the version number increases. Notrecommended for normal use. Use `authorize_upgrade` instead.This call requires Root origin.".to_string()),
					("apply_authorized_upgrade".to_string(), "Provide the preimage (runtime binary) `code` for an upgrade that has been authorized.If the authorization required a version check, this call will ensure the spec nameremains unchanged and that the spec version has increased.Depending on the runtime's `OnSetCode` configuration, this function may directly applythe new `code` in the same block or attempt to schedule the upgrade.All origins are allowed.".to_string()),
				]
				.to_vec(),
			),
			0, // "remark" extrinsic
		).expect_info("pop call parachain --pallet System --extrinsic remark --args \"0x11\" --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Bob");
		let mut call_config = CallParachainCommand {
			pallet: Some("System".to_string()),
			extrinsic: None,
			args: vec![].to_vec(),
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: DEFAULT_URI.to_string(),
			skip_confirm: false,
			call_data: None,
		};
		let api = call_config.initialize_api_client(&mut cli).await?;
		call_config.configure(&api, &mut cli).await?;

		assert_eq!(call_config.pallet, Some("System".to_string()));
		assert_eq!(call_config.extrinsic, Some("remark".to_string()));
		assert_eq!(call_config.args, ["0x11".to_string()].to_vec());
		assert_eq!(call_config.url, Url::parse("wss://rpc1.paseo.popnetwork.xyz")?);
		assert_eq!(call_config.suri, "//Bob".to_string());
		assert_eq!(call_config.display(), "pop call parachain --pallet System --extrinsic remark --args \"0x11\" --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Bob");
		cli.verify()
	}

	// This test only covers the interactive portion of the call parachain command selecting one of
	// the predefined actions, without actually submitting any extrinsic.
	#[tokio::test]
	async fn guide_user_to_configure_predefined_action_works() -> Result<()> {
		let mut call_config = CallParachainCommand {
			pallet: None,
			extrinsic: None,
			args: vec![].to_vec(),
			url: Url::parse(DEFAULT_URL)?,
			suri: DEFAULT_URI.to_string(),
			skip_confirm: false,
			call_data: None,
		};

		let mut cli = MockCli::new()
			.expect_intro("Call a parachain")
			.expect_input("Signer of the extrinsic:", "//Bob".into())
			.expect_input("Enter the value for the parameter: para_id", "2000".into())
			.expect_input("Enter the value for the parameter: max_amount", "10000".into())
			.expect_input("Which chain would you like to interact with?", "wss://polkadot-rpc.publicnode.com".into())
			.expect_select::<Pallet>(
				"What would you like to do?",
				Some(true),
				true,
				Some(
					[
						("Purchase on-demand coretime".to_string(), "OnDemand".to_string()),
                        ("Transfer Balance".to_string(), "Balances".to_string()),
                        ("All".to_string(), "Explore all pallets and extrinsics".to_string()),
					]
					.to_vec(),
				),
				0, // "Purchase on-demand coretime" action
			).expect_info("pop call parachain --pallet OnDemand --extrinsic place_order_allow_death --args \"10000\" \"2000\" --url wss://polkadot-rpc.publicnode.com/ --suri //Bob");

		let api = call_config.initialize_api_client(&mut cli).await?;
		call_config.configure(&api, &mut cli).await?;

		assert_eq!(call_config.pallet, Some("OnDemand".to_string()));
		assert_eq!(call_config.extrinsic, Some("place_order_allow_death".to_string()));
		assert_eq!(call_config.args, ["10000".to_string(), "2000".to_string()].to_vec());
		assert_eq!(call_config.url, Url::parse("wss://polkadot-rpc.publicnode.com")?);
		assert_eq!(call_config.suri, "//Bob".to_string());
		assert_eq!(call_config.display(), "pop call parachain --pallet OnDemand --extrinsic place_order_allow_death --args \"10000\" \"2000\" --url wss://polkadot-rpc.publicnode.com/ --suri //Bob");
		cli.verify()
	}

	#[tokio::test]
	async fn prepare_extrinsic_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let mut call_config = CallParachainCommand {
			pallet: None,
			extrinsic: None,
			args: vec!["0x11".to_string()].to_vec(),
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: DEFAULT_URI.to_string(),
			skip_confirm: false,
			call_data: None,
		};
		let mut cli = MockCli::new();
		// Error, no extrinsic specified.
		assert!(
			matches!(call_config.prepare_extrinsic(&api, &mut cli).await, Err(message) if message.to_string().contains("Please specify the extrinsic."))
		);
		call_config.extrinsic = Some("remark".to_string());
		// Error, no pallet specified.
		assert!(
			matches!(call_config.prepare_extrinsic(&api, &mut cli).await, Err(message) if message.to_string().contains("Please specify the pallet."))
		);
		call_config.pallet = Some("WrongName".to_string());
		// Error, no extrinsic specified.
		assert!(
			matches!(call_config.prepare_extrinsic(&api, &mut cli).await, Err(message) if message.to_string().contains("Metadata Error: Pallet with name WrongName not found"))
		);
		// Success, extrinsic and pallet specified.
		cli = MockCli::new().expect_info("Encoded call data: 0x00000411");
		call_config.pallet = Some("System".to_string());
		call_config.prepare_extrinsic(&api, &mut cli).await?;

		cli.verify()
	}

	#[tokio::test]
	async fn user_cancel_send_extrinsic_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let mut call_config = CallParachainCommand {
			pallet: Some("System".to_string()),
			extrinsic: Some("remark".to_string()),
			args: vec!["0x11".to_string()].to_vec(),
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: DEFAULT_URI.to_string(),
			skip_confirm: false,
			call_data: None,
		};
		let mut cli = MockCli::new()
			.expect_confirm("Do you want to submit the extrinsic?", false)
			.expect_outro_cancel("Extrinsic not submitted. Operation canceled by the user.");
		let tx = call_config.prepare_extrinsic(&api, &mut cli).await?;
		call_config.send_extrinsic(api, tx, false, &mut cli).await?;
		call_config.extrinsic = Some("remark".to_string());

		cli.verify()
	}

	#[test]
	fn reset_for_new_call_works() -> Result<()> {
		let mut call_config = CallParachainCommand {
			pallet: Some("System".to_string()),
			extrinsic: Some("remark".to_string()),
			args: vec!["0x11".to_string()].to_vec(),
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: DEFAULT_URI.to_string(),
			skip_confirm: false,
			call_data: None,
		};
		call_config.reset_for_new_call();
		assert_eq!(call_config.pallet, None);
		assert_eq!(call_config.extrinsic, None);
		assert_eq!(call_config.args.len(), 0);
		Ok(())
	}

	#[test]
	fn display_message_works() -> Result<()> {
		let mut cli = MockCli::new().expect_outro(&"Call completed successfully!");
		display_message("Call completed successfully!", true, &mut cli)?;
		cli.verify()?;
		let mut cli = MockCli::new().expect_outro_cancel("Call failed.");
		display_message("Call failed.", false, &mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn prompt_predefined_actions_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let pallets = parse_chain_metadata(&api).await?;
		let mut cli = MockCli::new().expect_select::<Pallet>(
			"What would you like to do?",
			Some(true),
			true,
			Some(
				[
					("Create an Asset".to_string(), "Assets".to_string()),
					("Mint an Asset".to_string(), "Assets".to_string()),
					("Create an NFT Collection".to_string(), "Nfts".to_string()),
					("Mint an NFT".to_string(), "Nfts".to_string()),
					("Transfer Balance".to_string(), "Balances".to_string()),
					("All".to_string(), "Explore all pallets and extrinsics".to_string()),
				]
				.to_vec(),
			),
			1, // "Mint an Asset" action
		);
		let action = prompt_predefined_actions(&pallets, &mut cli).await?;
		assert_eq!(action, Some(Action::MintAsset));
		cli.verify()
	}

	#[tokio::test]
	async fn prompt_for_param_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let pallets = parse_chain_metadata(&api).await?;
		// Using NFT mint extrinsic to test the majority of subfunctions
		let extrinsic = find_extrinsic_by_name(&pallets, "Nfts", "mint").await?;
		let mut cli = MockCli::new()
			.expect_input("Enter the value for the parameter: mint_price", "1000".into())
			.expect_input(
				"Enter the value for the parameter: Id",
				"5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".into(),
			)
			.expect_input("Enter the value for the parameter: item", "0".into())
			.expect_input("Enter the value for the parameter: collection", "0".into())
			.expect_select::<Pallet>(
				"Select the value for the parameter: mint_to",
				Some(true),
				true,
				Some(
					[
						("Id".to_string(), "".to_string()),
						("Index".to_string(), "".to_string()),
						("Raw".to_string(), "".to_string()),
						("Address32".to_string(), "".to_string()),
						("Address20".to_string(), "".to_string()),
					]
					.to_vec(),
				),
				0, // "Id" action
			)
			.expect_confirm(
				"Do you want to provide a value for the optional parameter: mint_price?",
				true,
			)
			.expect_confirm(
				"Do you want to provide a value for the optional parameter: owned_item?",
				false,
			)
			.expect_confirm(
				"Do you want to provide a value for the optional parameter: witness_data?",
				true,
			);
		// Test all the extrinsic params
		let mut params: Vec<String> = Vec::new();
		for param in extrinsic.params {
			params.push(prompt_for_param(&api, &mut cli, &param)?);
		}
		assert_eq!(params.len(), 4);
		assert_eq!(params[0], "0".to_string()); // collection: test primitive
		assert_eq!(params[1], "0".to_string()); // item: test primitive
		assert_eq!(params[2], "Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string()); // mint_to: test variant
		assert_eq!(params[3], "Some({owned_item: None(), mint_price: Some(1000)})".to_string()); // witness_data: test composite
		cli.verify()?;

		// Using Scheduler set_retry extrinsic to test the tuple params
		let extrinsic = find_extrinsic_by_name(&pallets, "Scheduler", "set_retry").await?;
		let mut cli = MockCli::new()
			.expect_input("Enter the value for the parameter: period", "0".into())
			.expect_input("Enter the value for the parameter: retries", "0".into())
			.expect_input(
				"Enter the value for the parameter: Index 1 of the tuple task",
				"0".into(),
			)
			.expect_input(
				"Enter the value for the parameter: Index 0 of the tuple task",
				"0".into(),
			);

		// Test all the extrinsic params
		let mut params: Vec<String> = Vec::new();
		for param in extrinsic.params {
			params.push(prompt_for_param(&api, &mut cli, &param)?);
		}
		assert_eq!(params.len(), 3);
		assert_eq!(params[0], "(0, 0)".to_string()); // task: test tuples
		assert_eq!(params[1], "0".to_string()); // retries: test primitive
		assert_eq!(params[2], "0".to_string()); // period: test primitive
		cli.verify()
	}
}
