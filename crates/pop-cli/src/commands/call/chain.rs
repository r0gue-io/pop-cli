// SPDX-License-Identifier: GPL-3.0

use std::path::Path;

use crate::{
	cli::{self, traits::*},
	common::{
		chain::{self, Chain},
		prompt::display_message,
		urls,
		wallet::{self, prompt_to_use_wallet},
	},
};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_chains::{
	construct_extrinsic, construct_sudo_extrinsic, decode_call_data, encode_call_data,
	find_dispatchable_by_name, find_pallet_by_name, sign_and_submit_extrinsic, supported_actions,
	Action, CallData, DynamicPayload, Function, OnlineClient, Pallet, Param, Payload,
	SubstrateConfig,
};
use url::Url;

const DEFAULT_URI: &str = "//Alice";
const ENCODED_CALL_DATA_MAX_LEN: usize = 500; // Maximum length of encoded call data to display.

/// Command to construct and execute extrinsics with configurable pallets, functions, arguments, and
/// signing options.
#[derive(Args, Clone, Default)]
pub struct CallChainCommand {
	/// The pallet containing the dispatchable function to execute.
	#[arg(short, long, value_parser = parse_pallet_name)]
	pallet: Option<String>,
	/// The dispatchable function to execute within the specified pallet.
	#[arg(short, long, value_parser = parse_function_name)]
	function: Option<String>,
	/// The dispatchable function arguments, encoded as strings.
	#[arg(short, long, num_args = 0..,)]
	args: Vec<String>,
	/// Websocket endpoint of a node.
	#[arg(short, long, value_parser)]
	url: Option<Url>,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[arg(short, long)]
	suri: Option<String>,
	/// Use a browser extension wallet to sign the extrinsic.
	#[arg(
		name = "use-wallet",
		short = 'w',
		long,
		default_value = "false",
		conflicts_with = "suri"
	)]
	use_wallet: bool,
	/// SCALE encoded bytes representing the call data of the extrinsic.
	#[arg(name = "call", short, long, conflicts_with_all = ["pallet", "function", "args"])]
	call_data: Option<String>,
	/// Authenticates the sudo key and dispatches a function call with `Root` origin.
	#[arg(short = 'S', long)]
	sudo: bool,
	/// Automatically signs and submits the extrinsic without prompting for confirmation.
	#[arg(short = 'y', long)]
	skip_confirm: bool,
}

impl CallChainCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		let mut cli = cli::Cli;
		cli.intro("Call a chain")?;
		// Check if all fields are specified via the command line.
		let prompt_to_repeat_call = self.requires_user_input();
		// Configure the chain.
		let chain = chain::configure(
			"Which chain would you like to interact with?",
			urls::LOCAL,
			&self.url,
			&mut cli,
		)
		.await?;
		// Execute the call if call_data is provided.
		if let Some(call_data) = self.call_data.as_ref() {
			if let Err(e) = self
				.submit_extrinsic_from_call_data(
					&chain.client,
					&chain.url,
					call_data,
					&mut cli::Cli,
				)
				.await
			{
				display_message(&e.to_string(), false, &mut cli::Cli)?;
			}
			return Ok(());
		}
		loop {
			// Configure the call based on command line arguments/call UI.
			let mut call = match self.configure_call(&chain, &mut cli) {
				Ok(call) => call,
				Err(e) => {
					display_message(&e.to_string(), false, &mut cli)?;
					break;
				},
			};
			// Display the configured call.
			cli.info(call.display(&chain))?;
			// Prepare the extrinsic.
			let xt = match call.prepare_extrinsic(&chain.client, &mut cli) {
				Ok(payload) => payload,
				Err(e) => {
					display_message(&e.to_string(), false, &mut cli)?;
					break;
				},
			};

			// Sign and submit the extrinsic.
			let result = if self.use_wallet {
				let call_data = xt.encode_call_data(&chain.client.metadata())?;
				wallet::submit_extrinsic(&chain.client, &chain.url, call_data, &mut cli)
					.await
					.map(|_| ()) // Mapping to `()` since we don't need events returned
			} else {
				call.submit_extrinsic(&chain.client, &chain.url, xt, &mut cli).await
			};

			if let Err(e) = result {
				display_message(&e.to_string(), false, &mut cli)?;
				break;
			}

			if !prompt_to_repeat_call ||
				!cli.confirm("Do you want to perform another call?")
					.initial_value(false)
					.interact()?
			{
				display_message("Call complete.", true, &mut cli)?;
				break;
			}
			self.reset_for_new_call();
		}
		Ok(())
	}

	// Configure the call based on command line arguments/call UI.
	fn configure_call(&mut self, chain: &Chain, cli: &mut impl Cli) -> Result<Call> {
		loop {
			// Resolve pallet.
			let pallet = match self.pallet {
				Some(ref pallet_name) => find_pallet_by_name(&chain.pallets, pallet_name)?,
				None => {
					// Specific predefined actions first.
					if let Some(action) = prompt_predefined_actions(&chain.pallets, cli)? {
						self.function = Some(action.function_name().to_string());
						find_pallet_by_name(&chain.pallets, action.pallet_name())?
					} else {
						let mut prompt = cli.select("Select the pallet to call:");
						for pallet_item in &chain.pallets {
							prompt = prompt.item(pallet_item, &pallet_item.name, &pallet_item.docs);
						}
						prompt.interact()?
					}
				},
			};

			// Resolve dispatchable function.
			let function = match self.function {
				Some(ref name) => find_dispatchable_by_name(&chain.pallets, &pallet.name, name)?,
				None => {
					let mut prompt = cli.select("Select the function to call:");
					for function in &pallet.functions {
						prompt = prompt.item(function, &function.name, &function.docs);
					}
					prompt.interact()?
				},
			};
			// Certain dispatchable functions are not supported yet due to complexity.
			if !function.is_supported {
				cli.outro_cancel(
					"The selected function is not supported yet. Please choose another one.",
				)?;
				self.reset_for_new_call();
				continue;
			}

			// Resolve dispatchable function arguments.
			let args = if self.args.is_empty() {
				let mut args = Vec::new();
				for param in &function.params {
					let input = prompt_for_param(cli, param)?;
					args.push(input);
				}
				args
			} else {
				self.expand_file_arguments()?
			};

			// If chain has sudo prompt the user to confirm if they want to execute the call via
			// sudo.
			if self.sudo {
				self.check_sudo(chain, cli)?;
			}

			let (use_wallet, suri) = self.determine_signing_method(cli)?;
			self.use_wallet = use_wallet;

			return Ok(Call {
				function: function.clone(),
				args,
				suri,
				skip_confirm: self.skip_confirm,
				sudo: self.sudo,
				use_wallet: self.use_wallet,
			});
		}
	}

	// Submits an extrinsic to the chain using the provided encoded call data.
	async fn submit_extrinsic_from_call_data(
		&self,
		client: &OnlineClient<SubstrateConfig>,
		url: &Url,
		call_data: &str,
		cli: &mut impl Cli,
	) -> Result<()> {
		let (use_wallet, suri) = self.determine_signing_method(cli)?;

		// Perform signing steps with wallet integration and return early.
		if use_wallet {
			let call_data_bytes =
				decode_call_data(call_data).map_err(|err| anyhow!("{}", format!("{err:?}")))?;
			wallet::submit_extrinsic(client, url, call_data_bytes, cli)
				.await
				.map_err(|err| anyhow!("{}", format!("{err:?}")))?;
			display_message("Call complete.", true, cli)?;
			return Ok(());
		}
		cli.info(format!("Encoded call data: {}", call_data))?;
		if !self.skip_confirm &&
			!cli.confirm("Do you want to submit the extrinsic?")
				.initial_value(true)
				.interact()?
		{
			display_message(
				&format!("Extrinsic with call data {call_data} was not submitted."),
				false,
				cli,
			)?;
			return Ok(());
		}
		let spinner = cliclack::spinner();
		spinner.start("Signing and submitting the extrinsic and then waiting for finalization, please be patient...");
		let call_data_bytes =
			decode_call_data(call_data).map_err(|err| anyhow!("{}", format!("{err:?}")))?;
		let result = sign_and_submit_extrinsic(client, url, CallData::new(call_data_bytes), &suri)
			.await
			.map_err(|err| anyhow!("{}", format!("{err:?}")))?;

		spinner.stop(result);
		display_message("Call complete.", true, cli)?;
		Ok(())
	}

	// Resolve who is signing the extrinsic. If a `suri` was provided via the command line,
	// skip the prompt.
	fn determine_signing_method(&self, cli: &mut impl Cli) -> Result<(bool, String)> {
		let mut use_wallet = self.use_wallet;
		let suri = match self.suri.as_ref() {
			Some(suri) => suri.clone(),
			None =>
				if !self.use_wallet {
					if prompt_to_use_wallet(cli)? {
						use_wallet = true;
						DEFAULT_URI.to_string()
					} else {
						cli.input("Signer of the extrinsic:")
							.default_input(DEFAULT_URI)
							.interact()?
					}
				} else {
					DEFAULT_URI.to_string()
				},
		};
		Ok((use_wallet, suri))
	}

	// Checks if the chain has the Sudo pallet and prompts the user to confirm if they want to
	// execute the call via `sudo`.
	fn check_sudo(&mut self, chain: &Chain, cli: &mut impl Cli) -> Result<()> {
		match find_dispatchable_by_name(&chain.pallets, "Sudo", "sudo") {
			Ok(_) => {
				if !self.skip_confirm {
					self.sudo = cli
                        .confirm(
                            "Are you sure you want to dispatch this function call with `Root` origin?",
                        )
                        .initial_value(true)
                        .interact()?;
				}
				Ok(())
			},
			Err(_) =>
				Err(anyhow::anyhow!("The sudo pallet is not supported by the chain. Aborting...")),
		}
	}

	// Resets specific fields to default values for a new call.
	fn reset_for_new_call(&mut self) {
		self.pallet = None;
		self.function = None;
		self.args.clear();
		self.sudo = false;
		self.use_wallet = false;
	}

	// Function to check if all required fields are specified.
	fn requires_user_input(&self) -> bool {
		self.pallet.is_none() ||
			self.function.is_none() ||
			self.args.is_empty() ||
			self.url.is_none() ||
			self.suri.is_none()
	}

	/// Replaces file arguments with their contents, leaving other arguments unchanged.
	fn expand_file_arguments(&self) -> Result<Vec<String>> {
		self.args
			.iter()
			.map(|arg| {
				if std::fs::metadata(arg).map(|m| m.is_file()).unwrap_or(false) {
					std::fs::read_to_string(arg)
						.map_err(|err| anyhow!("Failed to read file {}", err.to_string()))
				} else {
					Ok(arg.clone())
				}
			})
			.collect()
	}
}

/// Represents a configured dispatchable function call, including the pallet, function, arguments,
/// and signing options.
#[derive(Clone, Default)]
pub(crate) struct Call {
	/// The dispatchable function to execute.
	pub(crate) function: Function,
	/// The dispatchable function arguments, encoded as strings.
	pub(crate) args: Vec<String>,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	pub(crate) suri: String,
	/// Whether to use your browser wallet to sign the extrinsic.
	pub(crate) use_wallet: bool,
	/// Whether to automatically sign and submit the extrinsic without prompting for confirmation.
	pub(crate) skip_confirm: bool,
	/// Whether to dispatch the function call with `Root` origin.
	pub(crate) sudo: bool,
}

impl Call {
	// Prepares the extrinsic.
	pub(crate) fn prepare_extrinsic(
		&self,
		client: &OnlineClient<SubstrateConfig>,
		cli: &mut impl Cli,
	) -> Result<DynamicPayload> {
		let xt = match construct_extrinsic(&self.function, self.args.clone()) {
			Ok(tx) => tx,
			Err(e) => {
				return Err(anyhow!("Error: {}", e));
			},
		};
		// If sudo is required, wrap the call in a sudo call.
		let xt = if self.sudo { construct_sudo_extrinsic(xt) } else { xt };
		let encoded_data = encode_call_data(client, &xt)?;
		// If the encoded call data is too long, don't display it all.
		if encoded_data.len() < ENCODED_CALL_DATA_MAX_LEN {
			cli.info(format!("Encoded call data: {}", encode_call_data(client, &xt)?))?;
		}
		Ok(xt)
	}

	// Sign and submit an extrinsic.
	async fn submit_extrinsic(
		&mut self,
		client: &OnlineClient<SubstrateConfig>,
		url: &Url,
		tx: DynamicPayload,
		cli: &mut impl Cli,
	) -> Result<()> {
		if !self.skip_confirm &&
			!cli.confirm("Do you want to submit the extrinsic?")
				.initial_value(true)
				.interact()?
		{
			display_message(
				&format!("Extrinsic for `{}` was not submitted.", self.function.name),
				false,
				cli,
			)?;
			return Ok(());
		}
		let spinner = cliclack::spinner();
		spinner.start("Signing and submitting the extrinsic and then waiting for finalization, please be patient...");
		let result = sign_and_submit_extrinsic(client, url, tx, &self.suri)
			.await
			.map_err(|err| anyhow!("{}", format!("{err:?}")))?;
		spinner.stop(result);
		Ok(())
	}

	fn display(&self, chain: &Chain) -> String {
		let mut full_message = "pop call chain".to_string();
		full_message.push_str(&format!(" --pallet {}", self.function.pallet));
		full_message.push_str(&format!(" --function {}", self.function));
		if !self.args.is_empty() {
			let args: Vec<_> = self
				.args
				.iter()
				.map(|a| {
					// If the argument is too long, don't show it all, truncate it.
					if a.len() > ENCODED_CALL_DATA_MAX_LEN {
						format!("\"{}...{}\"", &a[..20], &a[a.len() - 20..])
					} else {
						format!("\"{a}\"")
					}
				})
				.collect();
			full_message.push_str(&format!(" --args {}", args.join(" ")));
		}
		full_message.push_str(&format!(" --url {}", chain.url));
		if self.use_wallet {
			full_message.push_str(" --use-wallet");
		} else {
			full_message.push_str(&format!(" --suri {}", self.suri));
		}
		if self.sudo {
			full_message.push_str(" --sudo");
		}
		full_message
	}
}

// Prompts the user for some predefined actions.
fn prompt_predefined_actions(pallets: &[Pallet], cli: &mut impl Cli) -> Result<Option<Action>> {
	let mut predefined_action = cli.select("What would you like to do?");
	for action in supported_actions(pallets) {
		predefined_action = predefined_action.item(
			Some(action.clone()),
			action.description(),
			action.pallet_name(),
		);
	}
	predefined_action = predefined_action.item(None, "All", "Explore all pallets and functions");
	Ok(predefined_action.interact()?)
}

// Prompts the user for the value of a parameter.
fn prompt_for_param(cli: &mut impl Cli, param: &Param) -> Result<String> {
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
		let value = get_param_value(cli, param)?;
		Ok(format!("Some({})", value))
	} else {
		get_param_value(cli, param)
	}
}

// Resolves the value of a parameter based on its type.
fn get_param_value(cli: &mut impl Cli, param: &Param) -> Result<String> {
	if param.is_sequence {
		prompt_for_sequence_param(cli, param)
	} else if param.sub_params.is_empty() {
		prompt_for_primitive_param(cli, param)
	} else if param.is_variant {
		prompt_for_variant_param(cli, param)
	} else if param.is_tuple {
		prompt_for_tuple_param(cli, param)
	} else {
		prompt_for_composite_param(cli, param)
	}
}

// Prompt for the value when it is a sequence.
fn prompt_for_sequence_param(cli: &mut impl Cli, param: &Param) -> Result<String> {
	let input_value = cli
		.input(format!(
		"The value for `{}` might be too large to enter. You may enter the path to a file instead.",
		param.name
	))
		.placeholder(&format!(
			"Enter a value of type {} or provide a file path (e.g. /path/to/your/file)",
			param.type_name
		))
		.interact()?;
	if Path::new(&input_value).is_file() {
		return std::fs::read_to_string(&input_value)
			.map_err(|err| anyhow!("Failed to read file {}", err.to_string()));
	}
	Ok(input_value)
}

// Prompt for the value when it is a primitive.
fn prompt_for_primitive_param(cli: &mut impl Cli, param: &Param) -> Result<String> {
	Ok(cli
		.input(format!("Enter the value for the parameter: {}", param.name))
		.placeholder(&format!("Type required: {}", param.type_name))
		.interact()?)
}

// Prompt the user to select the value of the variant parameter and recursively prompt for nested
// fields. Output example: `Id(5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY)` for the `Id`
// variant.
fn prompt_for_variant_param(cli: &mut impl Cli, param: &Param) -> Result<String> {
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
			let field_value = prompt_for_param(cli, field_arg)?;
			field_values.push(field_value);
		}
		Ok(format!("{}({})", selected_variant.name, field_values.join(", ")))
	} else {
		Ok(format!("{}()", selected_variant.name))
	}
}

// Recursively prompt the user for all the nested fields in a composite type.
// Example of a composite definition:
// Param {
//     name: "Id",
//     type_name: "AccountId32 ([u8;32])",
//     is_optional: false,
//     sub_params: [
//         Param {
//             name: "Id",
//             type_name: "[u8;32]",
//             is_optional: false,
//             sub_params: [],
//             is_variant: false
//         }
//     ],
//     is_variant: false
// }
fn prompt_for_composite_param(cli: &mut impl Cli, param: &Param) -> Result<String> {
	let mut field_values = Vec::new();
	for field_arg in &param.sub_params {
		let field_value = prompt_for_param(cli, field_arg)?;
		if param.sub_params.len() == 1 && param.name == param.sub_params[0].name {
			field_values.push(field_value);
		} else {
			field_values.push(format!("{}: {}", field_arg.name, field_value));
		}
	}
	if param.sub_params.len() == 1 && param.name == param.sub_params[0].name {
		Ok(field_values.join(", ").to_string())
	} else {
		Ok(format!("{{{}}}", field_values.join(", ")))
	}
}

// Recursively prompt the user for the tuple values.
fn prompt_for_tuple_param(cli: &mut impl Cli, param: &Param) -> Result<String> {
	let mut tuple_values = Vec::new();
	for tuple_param in param.sub_params.iter() {
		let tuple_value = prompt_for_param(cli, tuple_param)?;
		tuple_values.push(tuple_value);
	}
	Ok(format!("({})", tuple_values.join(", ")))
}

// Parser to capitalize the first letter of the pallet name.
fn parse_pallet_name(name: &str) -> Result<String, String> {
	let mut chars = name.chars();
	match chars.next() {
		Some(c) => Ok(c.to_ascii_uppercase().to_string() + chars.as_str()),
		None => Err("Pallet cannot be empty".to_string()),
	}
}

// Parser to convert the function name to lowercase.
fn parse_function_name(name: &str) -> Result<String, String> {
	Ok(name.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{cli::MockCli, common::wallet::USE_WALLET_PROMPT};
	use pop_chains::{parse_chain_metadata, set_up_client};
	use pop_common::test_env::TestNode;
	use tempfile::tempdir;
	use url::Url;

	const BOB_SURI: &str = "//Bob";

	#[tokio::test]
	async fn guide_user_to_call_chain_works() -> Result<()> {
		let node = TestNode::spawn().await?;
		let node_url = node.ws_url();
		let mut call_config = CallChainCommand {
			pallet: Some("System".to_string()),
			sudo: true,
			..Default::default()
		};

		let mut cli = MockCli::new()
		.expect_input("Which chain would you like to interact with?", node_url.into())
		.expect_select(
			"Select the function to call:",
			Some(true),
			true,
			Some(
				[
					("apply_authorized_upgrade".to_string(), "Provide the preimage (runtime binary) `code` for an upgrade that has been authorized. If the authorization required a version check, this call will ensure the spec name remains unchanged and that the spec version has increased. Depending on the runtime's `OnSetCode` configuration, this function may directly apply the new `code` in the same block or attempt to schedule the upgrade. All origins are allowed.".to_string()),
					("authorize_upgrade".to_string(), "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied later. This call requires Root origin.".to_string()),
					("authorize_upgrade_without_checks".to_string(), "Authorize an upgrade to a given `code_hash` for the runtime. The runtime can be supplied later. WARNING: This authorizes an upgrade that will take place without any safety checks, for example that the spec name remains the same and that the version number increases. Not recommended for normal use. Use `authorize_upgrade` instead. This call requires Root origin.".to_string()),
					("kill_prefix".to_string(), "Kill all storage items with a key that starts with the given prefix. **NOTE:** We rely on the Root origin to provide us the number of subkeys under the prefix we are removing to accurately calculate the weight of this function.".to_string()),
					("kill_storage".to_string(), "Kill some items from storage.".to_string()),
					("remark".to_string(), "Make some on-chain remark. Can be executed by every `origin`.".to_string()),
					("remark_with_event".to_string(), "Make some on-chain remark and emit event.".to_string()),
					("set_code".to_string(), "Set the new runtime code.".to_string()),
					("set_code_without_checks".to_string(), "Set the new runtime code without doing any checks of the given `code`. Note that runtime upgrades will not run if this is called with a not-increasing spec version!".to_string()),
					("set_heap_pages".to_string(), "Set the number of pages in the WebAssembly environment's heap.".to_string()),
					("set_storage".to_string(), "Set some items of storage.".to_string()),
				]
				.to_vec(),
			),
			5, // "remark" dispatchable function
            None,
        )
            .expect_input("The value for `remark` might be too large to enter. You may enter the path to a file instead.", "0x11".into())
            .expect_confirm("Are you sure you want to dispatch this function call with `Root` origin?", true)
            .expect_confirm(USE_WALLET_PROMPT, true);

		let chain = chain::configure(
			"Which chain would you like to interact with?",
			node_url,
			&None,
			&mut cli,
		)
		.await?;
		assert_eq!(chain.url, Url::parse(node_url)?);

		let call_chain = call_config.configure_call(&chain, &mut cli)?;
		assert_eq!(call_chain.function.pallet, "System");
		assert_eq!(call_chain.function.name, "remark");
		assert_eq!(call_chain.args, ["0x11".to_string()].to_vec());
		assert_eq!(call_chain.suri, "//Alice"); // Default value
		assert!(call_chain.use_wallet);
		assert!(call_chain.sudo);
		assert_eq!(call_chain.display(&chain), format!("pop call chain --pallet System --function remark --args \"0x11\" --url {node_url}/ --use-wallet --sudo"));
		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_configure_predefined_action_works() -> Result<()> {
		let node = TestNode::spawn().await?;
		let node_url = node.ws_url();
		let mut call_config = CallChainCommand::default();

		let mut cli = MockCli::new()
			.expect_input("Which chain would you like to interact with?", node_url.into());
		let chain = chain::configure(
			"Which chain would you like to interact with?",
			node_url,
			&None,
			&mut cli,
		)
		.await?;
		assert_eq!(chain.url, Url::parse(node_url)?);
		cli.verify()?;

		let mut cli = MockCli::new()
			.expect_select(
				"What would you like to do?",
				Some(true),
				true,
				Some(
					supported_actions(&chain.pallets)
						.into_iter()
						.map(|action| {
							(action.description().to_string(), action.pallet_name().to_string())
						})
						.chain(std::iter::once((
							"All".to_string(),
							"Explore all pallets and functions".to_string(),
						)))
						.collect::<Vec<_>>(),
				),
				1, // "Create an asset" action
				None,
			)
			.expect_input("Enter the value for the parameter: id", "10000".into())
			.expect_select(
				"Select the value for the parameter: admin",
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
				None,
			)
			.expect_input(
				"Enter the value for the parameter: Id",
				"5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".into(),
			)
			.expect_input("Enter the value for the parameter: min_balance", "2000".into())
			.expect_input("Signer of the extrinsic:", BOB_SURI.into());

		let call_chain = call_config.configure_call(&chain, &mut cli)?;

		assert_eq!(call_chain.function.pallet, "Assets");
		assert_eq!(call_chain.function.name, "create");
		assert_eq!(
			call_chain.args,
			[
				"10000".to_string(),
				"Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string(),
				"2000".to_string()
			]
			.to_vec()
		);
		assert_eq!(call_chain.suri, "//Bob");
		assert!(!call_chain.sudo);
		assert_eq!(call_chain.display(&chain), format!("pop call chain --pallet Assets --function create --args \"10000\" \"Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)\" \"2000\" --url {node_url}/ --suri //Bob"));
		cli.verify()
	}

	#[tokio::test]
	async fn prepare_extrinsic_works() -> Result<()> {
		let node = TestNode::spawn().await?;
		let node_url = node.ws_url();
		let client = set_up_client(node_url).await?;
		let mut call_config = Call {
			function: Function {
				pallet: "WrongName".to_string(),
				name: "WrongName".to_string(),
				..Default::default()
			},
			args: vec!["0x11".to_string()].to_vec(),
			suri: DEFAULT_URI.to_string(),
			use_wallet: false,
			skip_confirm: false,
			sudo: false,
		};
		let mut cli = MockCli::new();
		// Error, wrong name of the pallet.
		assert!(matches!(
				call_config.prepare_extrinsic(&client, &mut cli),
				Err(message)
					if message.to_string().contains("Failed to encode call data. Metadata Error: Pallet with name WrongName not found")));
		let pallets = parse_chain_metadata(&client)?;
		call_config.function.pallet = "System".to_string();
		// Error, wrong name of the function.
		assert!(matches!(
				call_config.prepare_extrinsic(&client, &mut cli),
				Err(message)
					if message.to_string().contains("Failed to encode call data. Metadata Error: Call with name WrongName not found")));
		// Success, pallet and dispatchable function specified.
		cli = MockCli::new().expect_info("Encoded call data: 0x00000411");
		call_config.function = find_dispatchable_by_name(&pallets, "System", "remark")?.clone();
		let xt = call_config.prepare_extrinsic(&client, &mut cli)?;
		assert_eq!(xt.call_name(), "remark");
		assert_eq!(xt.pallet_name(), "System");

		// Prepare extrinsic wrapped in sudo works.
		cli = MockCli::new().expect_info("Encoded call data: 0x070000000411");
		call_config.sudo = true;
		call_config.prepare_extrinsic(&client, &mut cli)?;

		cli.verify()
	}

	#[tokio::test]
	async fn user_cancel_submit_extrinsic_from_call_data_works() -> Result<()> {
		let node = TestNode::spawn().await?;
		let node_url = node.ws_url();
		let client = set_up_client(node_url).await?;
		let call_config = CallChainCommand {
			pallet: None,
			function: None,
			args: vec![].to_vec(),
			url: Some(Url::parse(node_url)?),
			suri: None,
			use_wallet: false,
			skip_confirm: false,
			call_data: Some("0x00000411".to_string()),
			sudo: false,
		};
		let mut cli = MockCli::new()
			.expect_confirm(USE_WALLET_PROMPT, false)
			.expect_input("Signer of the extrinsic:", "//Bob".into())
			.expect_confirm("Do you want to submit the extrinsic?", false)
			.expect_outro_cancel("Extrinsic with call data 0x00000411 was not submitted.");
		call_config
			.submit_extrinsic_from_call_data(
				&client,
				&Url::parse(node_url)?,
				"0x00000411",
				&mut cli,
			)
			.await?;

		cli.verify()
	}

	#[test]
	fn reset_for_new_call_works() -> Result<()> {
		let mut call_config = CallChainCommand {
			pallet: Some("System".to_string()),
			function: Some("remark".to_string()),
			args: vec!["0x11".to_string()].to_vec(),
			url: Some(Url::parse(urls::LOCAL)?),
			use_wallet: true,
			suri: Some(DEFAULT_URI.to_string()),
			skip_confirm: false,
			call_data: None,
			sudo: true,
		};
		call_config.reset_for_new_call();
		assert_eq!(call_config.pallet, None);
		assert_eq!(call_config.function, None);
		assert_eq!(call_config.args.len(), 0);
		assert!(!call_config.sudo);
		assert!(!call_config.use_wallet);
		Ok(())
	}

	#[test]
	fn requires_user_input_works() -> Result<()> {
		let mut call_config = CallChainCommand {
			pallet: Some("System".to_string()),
			function: Some("remark".to_string()),
			args: vec!["0x11".to_string()].to_vec(),
			url: Some(Url::parse(urls::LOCAL)?),
			suri: Some(DEFAULT_URI.to_string()),
			use_wallet: false,
			skip_confirm: false,
			call_data: None,
			sudo: false,
		};
		assert!(!call_config.requires_user_input());
		call_config.pallet = None;
		assert!(call_config.requires_user_input());
		Ok(())
	}

	#[test]
	fn expand_file_arguments_works() -> Result<()> {
		let mut call_config = CallChainCommand {
			pallet: Some("Registrar".to_string()),
			function: Some("register".to_string()),
			args: vec!["2000".to_string(), "0x1".to_string(), "0x12".to_string()].to_vec(),
			url: Some(Url::parse(urls::LOCAL)?),
			suri: Some(DEFAULT_URI.to_string()),
			use_wallet: false,
			call_data: None,
			skip_confirm: false,
			sudo: false,
		};
		assert_eq!(
			call_config.expand_file_arguments()?,
			vec!["2000".to_string(), "0x1".to_string(), "0x12".to_string()]
		);
		// Temporal file for testing when the input is a file.
		let temp_dir = tempdir()?;
		let genesis_file = temp_dir.path().join("genesis_file.json");
		std::fs::write(&genesis_file, "genesis_file_content")?;
		let wasm_file = temp_dir.path().join("wasm_file.json");
		std::fs::write(&wasm_file, "wasm_file_content")?;
		call_config.args = vec![
			"2000".to_string(),
			genesis_file.display().to_string(),
			wasm_file.display().to_string(),
		];
		assert_eq!(
			call_config.expand_file_arguments()?,
			vec![
				"2000".to_string(),
				"genesis_file_content".to_string(),
				"wasm_file_content".to_string()
			]
		);
		Ok(())
	}

	#[test]
	fn parse_pallet_name_works() -> Result<()> {
		assert_eq!(parse_pallet_name("system").unwrap(), "System");
		assert_eq!(parse_pallet_name("balances").unwrap(), "Balances");
		assert_eq!(parse_pallet_name("nfts").unwrap(), "Nfts");
		Ok(())
	}

	#[test]
	fn parse_function_name_works() -> Result<()> {
		assert_eq!(parse_function_name("Remark").unwrap(), "remark");
		assert_eq!(parse_function_name("Force_transfer").unwrap(), "force_transfer");
		assert_eq!(parse_function_name("MINT").unwrap(), "mint");
		Ok(())
	}
}
