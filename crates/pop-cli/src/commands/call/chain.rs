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
use anyhow::{Result, anyhow};
use clap::Args;
use pop_chains::{
	Action, CallData, CallItem, DynamicPayload, Function, OnlineClient, Pallet, Param, Payload,
	SubstrateConfig, construct_extrinsic, construct_sudo_extrinsic, decode_call_data,
	encode_call_data, find_callable_by_name, find_pallet_by_name, raw_value_to_string,
	render_storage_key_values, sign_and_submit_extrinsic, supported_actions, type_to_param,
};
use scale_info::PortableRegistry;
use serde::Serialize;
use url::Url;

const DEFAULT_URI: &str = "//Alice";
const ENCODED_CALL_DATA_MAX_LEN: usize = 500; // Maximum length of encoded call data to display.

fn to_tuple(args: &[String]) -> String {
	if args.len() < 2 {
		panic!("Cannot convert to tuple: too few arguments");
	}
	format!("({})", args.join(","))
}

/// Command to construct and execute extrinsics with configurable pallets, functions, arguments, and
/// signing options.
#[derive(Args, Clone, Default, Serialize)]
pub struct CallChainCommand {
	/// The pallet containing the dispatchable function to execute.
	#[arg(short, long, value_parser = parse_pallet_name)]
	pub pallet: Option<String>,
	/// The dispatchable function, storage item, or constant to execute/query within the specified
	/// pallet. It must match the exact name as in the source code.
	#[arg(short, long)]
	pub function: Option<String>,
	/// The dispatchable function arguments, encoded as strings.
	#[arg(short, long, num_args = 0..,)]
	pub args: Vec<String>,
	/// Websocket endpoint of a node.
	#[arg(short, long, value_parser)]
	pub url: Option<Url>,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[serde(skip_serializing)]
	#[arg(short, long)]
	pub suri: Option<String>,
	/// Use a browser extension wallet to sign the extrinsic.
	#[arg(
		name = "use-wallet",
		short = 'w',
		long,
		default_value = "false",
		conflicts_with = "suri"
	)]
	pub use_wallet: bool,
	/// SCALE encoded bytes representing the call data of the extrinsic.
	#[arg(name = "call", short, long, conflicts_with_all = ["pallet", "function", "args"])]
	pub call_data: Option<String>,
	/// Authenticates the sudo key and dispatches a function call with `Root` origin.
	#[arg(short = 'S', long)]
	pub sudo: bool,
	/// Automatically signs and submits the extrinsic without prompting for confirmation.
	#[arg(short = 'y', long)]
	pub skip_confirm: bool,
	/// Display chain metadata instead of executing a call.
	/// Use alone to list all pallets, or with --pallet to show pallet details.
	#[arg(short = 'm', long, conflicts_with_all = ["function", "args", "suri", "use-wallet", "call", "sudo"])]
	pub metadata: bool,
}

impl CallChainCommand {
	/// Executes the command.
	pub async fn execute(mut self) -> Result<()> {
		let mut cli = cli::Cli;
		cli.intro("Call a chain")?;
		// Configure the chain.
		let chain = chain::configure(
			"Select a chain (type to filter)",
			"Which chain would you like to interact with?",
			urls::LOCAL,
			&self.url,
			|_| true,
			&mut cli,
		)
		.await?;

		// Handle metadata display mode
		if self.metadata {
			return self.display_metadata(&chain, &mut cli);
		}

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
			match call.function {
				CallItem::Function(_) => {
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
				},
				CallItem::Constant(constant) => {
					// We already have the value of a constant, so we don't need to query it
					cli.success(&raw_value_to_string(&constant.value, "")?)?;
				},
				CallItem::Storage(ref storage) => {
					// Parse string arguments to Value types for storage query
					let keys = if !call.args.is_empty() {
						// Storage map with keys - need to parse and prepare them
						if let Some(key_ty) = storage.key_id {
							// Get metadata to convert type_id to Param for parsing
							let metadata = chain.client.metadata();
							let registry = metadata.types();
							let type_info = registry
								.resolve(key_ty)
								.ok_or(anyhow!("Failed to resolve storage key type: {key_ty}"))?;
							let name = type_info
								.path
								.segments
								.last()
								.unwrap_or(&"".to_string())
								.to_string();

							// Convert the key type_id to a Param for parsing
							let key_param = type_to_param(&name, registry, key_ty)
								.map_err(|e| anyhow!("Failed to parse storage key type: {e}"))?;

							// Parse the string arguments into Value types
							pop_chains::parse_dispatchable_arguments(
								&[key_param],
								call.args.clone(),
							)
							.map_err(|e| anyhow!("Failed to parse storage arguments: {e}"))?
						} else {
							// StorageValue - no keys needed
							vec![]
						}
					} else {
						// No arguments needed
						vec![]
					};

					// Query the storage
					if storage.query_all {
						match storage.query_all(&chain.client, keys).await {
							Ok(values) => {
								cli.success(&render_storage_key_values(values.as_slice())?)?;
							},
							Err(e) => {
								cli.error(format!("Failed to query storage: {e}"))?;
								break;
							},
						}
					} else {
						match storage.query(&chain.client, keys.clone()).await {
							Ok(Some(value)) => {
								let result = vec![(keys, value)];
								cli.success(&render_storage_key_values(result.as_slice())?)?;
							},
							Ok(None) => {
								cli.warning("Storage value not found")?;
							},
							Err(e) => {
								cli.error(format!("Failed to query storage: {e}"))?;
								break;
							},
						}
					}
				},
			};

			if self.skip_confirm ||
				!cli.confirm("Do you want to perform another call?")
					.initial_value(true)
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
	pub fn configure_call(&mut self, chain: &Chain, cli: &mut impl Cli) -> Result<Call> {
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
						let mut prompt = cli.select("Select the pallet to call (type to filter)");
						for pallet_item in &chain.pallets {
							prompt = prompt.item(pallet_item, &pallet_item.name, &pallet_item.docs);
						}
						prompt.filter_mode().interact()?
					}
				},
			};

			// Resolve dispatchable function.
			let mut call_item = match self.function {
				Some(ref name) => find_callable_by_name(&chain.pallets, &pallet.name, name)?,
				None => {
					let mut prompt = cli.select("Select the function to call (type to filter)");
					for callable in pallet.get_all_callables() {
						let name = format!("{} {}", callable.hint(), callable);
						let docs = callable.docs();
						prompt = prompt.item(callable.clone(), &name, docs);
					}
					prompt.filter_mode().interact()?
				},
			};

			let (args, suri) = match &mut call_item {
				CallItem::Function(function) => {
					// Certain dispatchable functions are not supported yet due to complexity.
					if !function.is_supported {
						cli.outro_cancel(
							"The selected function is not supported yet. Please choose another one.",
						)?;
						self.reset_for_new_call();
						continue;
					}

					// Resolve dispatchable function arguments.
					let args = self.resolve_function_args(function, cli)?;

					// If the chain has sudo prompt the user to confirm if they want to execute the
					// call via sudo.
					if self.sudo {
						self.check_sudo(chain, cli)?;
					}

					let (use_wallet, suri) = self.determine_signing_method(cli)?;
					self.use_wallet = use_wallet;
					(args, Some(suri))
				},
				CallItem::Storage(storage) => {
					// Handle storage queries - check if parameters are needed
					let args = if let Some(key_ty) = storage.key_id {
						// Storage map requires key parameters
						self.expand_file_arguments()?;
						// Get metadata to convert type_id to Param
						let metadata = chain.client.metadata();
						let registry = metadata.types();
						let type_info = registry
							.resolve(key_ty)
							.ok_or(anyhow!("Failed to resolve storage key type: {key_ty}"))?;
						let name =
							type_info.path.segments.last().unwrap_or(&"".to_string()).to_string();

						// Convert the key type_id to a Param for prompting
						let key_param = type_to_param(&name.to_string(), registry, key_ty)
							.map_err(|e| anyhow!("Failed to parse storage key type: {e}"))?;

						let is_composite = key_param.sub_params.len() > 1;
						let (mut params, len) =
							if self.args.len() == key_param.sub_params.len() && is_composite {
								(vec![to_tuple(self.args.as_slice())], self.args.len())
							} else if self.args.len() == 1 && is_composite {
								// Handle composite tuple string like "(A, B, C)"
								let arg = self.args[0]
									.trim()
									.trim_start_matches("(")
									.trim_start_matches("[")
									.trim_end_matches(")")
									.trim_end_matches("]")
									.to_string();
								let len = arg
									.split(',')
									.map(|s| s.trim().to_string())
									.collect::<Vec<_>>()
									.len();
								(self.args.clone(), len)
							} else {
								(self.args.clone(), self.args.len())
							};
						if key_param.sub_params.is_empty() && params.is_empty() {
							// Prompt user for the storage key
							let key_value = prompt_for_param(cli, &key_param, false)?;
							if !key_value.is_empty() {
								params.push(key_value);
							} else {
								storage.query_all = true;
							}
						} else {
							for (pos, sub_param) in
								key_param.sub_params.iter().enumerate().skip(len)
							{
								let required = is_composite && len + pos > 0;
								let sub_key_value = prompt_for_param(cli, sub_param, required)?;
								if !sub_key_value.is_empty() {
									params.push(sub_key_value);
								} else {
									storage.query_all = true;
									break;
								}
							}
							if is_composite && params.len() > 1 {
								params = vec![to_tuple(params.as_slice())];
							}
						}
						params
					} else {
						// Plain storage - no parameters needed
						vec![]
					};

					// Storage queries don't require signing
					(args, None)
				},
				// Constants don't require parameters
				CallItem::Constant(_) => (vec![], None),
			};

			return Ok(Call {
				function: call_item,
				args,
				suri,
				skip_confirm: self.skip_confirm,
				sudo: self.sudo,
				use_wallet: self.use_wallet,
			});
		}
	}

	// Submits an extrinsic to the chain using the provided encoded call data.
	pub async fn submit_extrinsic_from_call_data(
		&self,
		client: &OnlineClient<SubstrateConfig>,
		url: &Url,
		call_data: &str,
		cli: &mut impl Cli,
	) -> Result<()> {
		let (use_wallet, suri) = self.determine_signing_method(cli)?;

		// Perform signing steps with wallet integration and return early.
		if use_wallet {
			let call_data_bytes = decode_call_data(call_data).map_err(|err| anyhow!("{err:?}"))?;
			wallet::submit_extrinsic(client, url, call_data_bytes, cli)
				.await
				.map_err(|err| anyhow!("{err:?}"))?;
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
		spinner.start(
			"Signing and submitting the extrinsic and then waiting for finalization, please be patient...",
		);
		let call_data_bytes = decode_call_data(call_data).map_err(|err| anyhow!("{err:?}"))?;
		let result = sign_and_submit_extrinsic(client, url, CallData::new(call_data_bytes), &suri)
			.await
			.map_err(|err| anyhow!("{err:?}"))?;

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
			None => {
				if self.skip_confirm && !self.use_wallet {
					anyhow::bail!(
						"When skipping confirmation, a signer must be provided via --use-wallet or --suri."
					)
				}
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
				}
			},
		};
		Ok((use_wallet, suri))
	}

	// Checks if the chain has the Sudo pallet and prompts the user to confirm if they want to
	// execute the call via `sudo`.
	fn check_sudo(&mut self, chain: &Chain, cli: &mut impl Cli) -> Result<()> {
		match find_callable_by_name(&chain.pallets, "Sudo", "sudo") {
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

	/// Displays chain metadata (pallets, calls, storage, constants).
	pub fn display_metadata(&self, chain: &Chain, cli: &mut impl Cli) -> Result<()> {
		match &self.pallet {
			// No pallet specified: list all pallets
			None => list_pallets(&chain.pallets, cli),
			// Pallet specified: show pallet details
			Some(pallet_name) => {
				let pallet = find_pallet_by_name(&chain.pallets, pallet_name)?;
				let metadata = chain.client.metadata();
				let registry = metadata.types();
				show_pallet(pallet, registry, cli)
			},
		}
	}

	/// Replaces file arguments with their contents, leaving other arguments unchanged.
	fn expand_file_arguments(&self) -> Result<Vec<String>> {
		self.args
			.iter()
			.map(|arg| {
				if std::fs::metadata(arg).map(|m| m.is_file()).unwrap_or(false) {
					std::fs::read_to_string(arg).map_err(|err| anyhow!("Failed to read file {err}"))
				} else {
					Ok(arg.clone())
				}
			})
			.collect()
	}

	/// Resolves dispatchable arguments by leveraging CLI-provided values when available,
	/// prompting for missing ones. Updates `self.args` with the resolved values.
	/// Returns an error if more arguments than expected are provided.
	fn resolve_function_args(
		&mut self,
		function: &Function,
		cli: &mut impl Cli,
	) -> Result<Vec<String>> {
		let expanded_args = self.expand_file_arguments()?;
		if expanded_args.len() > function.params.len() {
			return Err(anyhow!(
				"Expected {} arguments for `{}`, but received {}. Remove the extra values or run \
				 without `--args` to be prompted.",
				function.params.len(),
				function.name,
				expanded_args.len()
			));
		}

		let mut resolved_args = Vec::with_capacity(function.params.len());
		for (idx, param) in function.params.iter().enumerate() {
			if let Some(value) = expanded_args.get(idx) {
				resolved_args.push(value.clone());
			} else {
				resolved_args.push(prompt_for_param(cli, param, true)?);
			}
		}

		self.args = resolved_args.clone();
		Ok(resolved_args)
	}
}

/// Lists all pallets available on the chain.
pub fn list_pallets(pallets: &[Pallet], cli: &mut impl Cli) -> Result<()> {
	cli.info(format!("Available pallets ({}):\n", pallets.len()))?;
	for pallet in pallets {
		if pallet.docs.is_empty() {
			cli.plain(format!("  {}", pallet.name))?;
		} else {
			cli.plain(format!("  {} - {}", pallet.name, pallet.docs))?;
		}
	}
	Ok(())
}

/// Shows details of a specific pallet (calls, storage, constants).
pub fn show_pallet(pallet: &Pallet, registry: &PortableRegistry, cli: &mut impl Cli) -> Result<()> {
	cli.info(format!("Pallet: {}\n", pallet.name))?;
	if !pallet.docs.is_empty() {
		cli.plain(format!("{}\n", pallet.docs))?;
	}

	// Show calls/extrinsics with parameters and docs
	if !pallet.functions.is_empty() {
		cli.plain(format!(
			"\n━━━ Extrinsics ({}) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
			pallet.functions.len()
		))?;
		for func in &pallet.functions {
			let status = if func.is_supported { "" } else { " [NOT SUPPORTED]" };
			cli.plain(format!("\n  {}{}", func.name, status))?;
			// Format parameters on separate lines for readability
			if !func.params.is_empty() {
				cli.plain("    Parameters:".to_string())?;
				for param in &func.params {
					cli.plain(format!("      - {}: {}", param.name, param.type_name))?;
				}
			}
			if !func.docs.is_empty() {
				cli.plain(format!("    Description: {}", func.docs))?;
			}
		}
	}

	// Show storage with key/value info
	if !pallet.state.is_empty() {
		cli.plain(format!(
			"\n\n━━━ Storage ({}) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
			pallet.state.len()
		))?;
		for storage in &pallet.state {
			cli.plain(format!("\n  {}", storage.name))?;
			// Resolve and display key type for maps
			if let Some(key_id) = storage.key_id &&
				let Ok(key_param) = type_to_param("key", registry, key_id)
			{
				cli.plain(format!("    Key: {}", key_param.type_name))?;
			}
			// Resolve and display value type
			if let Ok(value_param) = type_to_param("value", registry, storage.type_id) {
				cli.plain(format!("    Value: {}", value_param.type_name))?;
			}
			if !storage.docs.is_empty() {
				cli.plain(format!("    Description: {}", storage.docs))?;
			}
		}
	}

	// Show constants with values and docs
	if !pallet.constants.is_empty() {
		cli.plain(format!(
			"\n\n━━━ Constants ({}) ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
			pallet.constants.len()
		))?;
		for constant in &pallet.constants {
			let value_str = raw_value_to_string(&constant.value, "").map_err(|e| {
				anyhow!("Failed to decode constant {}::{}: {e}", pallet.name, constant.name)
			})?;
			cli.plain(format!("\n  {}", constant.name))?;
			cli.plain(format!("    Value: {}", value_str))?;
			if !constant.docs.is_empty() {
				cli.plain(format!("    Description: {}", constant.docs))?;
			}
		}
	}

	Ok(())
}
/// Represents a configured dispatchable function call, including the pallet, function, arguments,
/// and signing options.
#[derive(Clone, Default)]
pub struct Call {
	/// The callable to execute. It can read from storage or execute an extrinsic.
	pub function: CallItem,
	/// The dispatchable function arguments, encoded as strings.
	pub args: Vec<String>,
	/// Secret key URI for the account signing the extrinsic.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	pub suri: Option<String>,
	/// Whether to use your browser wallet to sign the extrinsic.
	pub use_wallet: bool,
	/// Whether to automatically sign and submit the extrinsic without prompting for confirmation.
	pub skip_confirm: bool,
	/// Whether to dispatch the function call with `Root` origin.
	pub sudo: bool,
}

impl Call {
	// Prepares the extrinsic.
	pub fn prepare_extrinsic(
		&self,
		client: &OnlineClient<SubstrateConfig>,
		cli: &mut impl Cli,
	) -> Result<DynamicPayload> {
		let function = self
			.function
			.as_function()
			.ok_or(anyhow!("Error: The call is not an extrinsic call"))?;
		let xt = match construct_extrinsic(function, self.args.clone()) {
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
		let function = self
			.function
			.as_function()
			.ok_or(anyhow!("Error: The call is not an extrinsic call"))?;
		if !self.skip_confirm &&
			!cli.confirm("Do you want to submit the extrinsic?")
				.initial_value(true)
				.interact()?
		{
			display_message(
				&format!("Extrinsic for `{}` was not submitted.", function.name),
				false,
				cli,
			)?;
			return Ok(());
		}
		let spinner = cliclack::spinner();
		spinner.start(
			"Signing and submitting the extrinsic and then waiting for finalization, please be patient...",
		);
		let suri = self.suri.clone().ok_or(anyhow!("Error: The secret key URI is missing"))?;
		let result = sign_and_submit_extrinsic(client, url, tx, &suri)
			.await
			.map_err(|err| anyhow!("{err:?}"))?;
		spinner.stop(result);
		Ok(())
	}

	pub fn display(&self, chain: &Chain) -> String {
		let mut full_message = "pop call chain".to_string();
		full_message.push_str(&format!(" --pallet {}", self.function.pallet()));
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
		} else if let Some(suri) = &self.suri {
			full_message.push_str(&format!(" --suri {suri}"));
		}
		if self.sudo {
			full_message.push_str(" --sudo");
		}
		if self.skip_confirm {
			full_message.push_str(" --skip-confirm");
		}
		full_message
	}
}

// Prompts the user for some predefined actions.
fn prompt_predefined_actions(pallets: &[Pallet], cli: &mut impl Cli) -> Result<Option<Action>> {
	let mut predefined_action = cli.select("What would you like to do?");
	predefined_action = predefined_action.item(None, "Other", "Explore all pallets and functions");
	for action in supported_actions(pallets) {
		predefined_action = predefined_action.item(
			Some(action.clone()),
			action.description(),
			action.pallet_name(),
		);
	}
	Ok(predefined_action.interact()?)
}

// Prompts the user for the value of a parameter.
fn prompt_for_param(cli: &mut impl Cli, param: &Param, force_required: bool) -> Result<String> {
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
		let value = get_param_value(cli, param, true)?;
		Ok(format!("Some({})", value))
	} else {
		get_param_value(cli, param, force_required)
	}
}

// Resolves the value of a parameter based on its type.
fn get_param_value(cli: &mut impl Cli, param: &Param, force_required: bool) -> Result<String> {
	if param.is_sequence {
		prompt_for_sequence_param(cli, param, force_required)
	} else if param.sub_params.is_empty() {
		prompt_for_primitive_param(cli, param, force_required)
	} else if param.is_variant {
		prompt_for_variant_param(cli, param, force_required)
	} else if param.is_tuple {
		prompt_for_tuple_param(cli, param, force_required)
	} else {
		prompt_for_composite_param(cli, param, force_required)
	}
}

// Prompt for the value when it is a sequence.
fn prompt_for_sequence_param(
	cli: &mut impl Cli,
	param: &Param,
	force_required: bool,
) -> Result<String> {
	let input_value = cli
		.input(format!(
			"The value for `{}` might be too large to enter. You may enter the path to a file instead.",
			param.name
		))
		.placeholder(&format!(
			"Enter a value of type {} or provide a file path (e.g. /path/to/your/file)",
			param.type_name
		))
		.required(param.is_optional || force_required)
		.interact()?;
	if Path::new(&input_value).is_file() {
		return std::fs::read_to_string(&input_value)
			.map_err(|err| anyhow!("Failed to read file {err}"));
	}
	Ok(input_value)
}

// Prompt for the value when it is a primitive.
fn prompt_for_primitive_param(
	cli: &mut impl Cli,
	param: &Param,
	force_required: bool,
) -> Result<String> {
	Ok(cli
		.input(format!("Enter the value for the parameter: {}", param.name))
		.placeholder(&format!("Type required: {}", param.type_name))
		.required(param.is_optional || force_required)
		.interact()?)
}

// Prompt the user to select the value of the variant parameter and recursively prompt for nested
// fields. Output example: `Id(5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY)` for the `Id`
// variant.
fn prompt_for_variant_param(
	cli: &mut impl Cli,
	param: &Param,
	force_required: bool,
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
			let field_value = prompt_for_param(cli, field_arg, force_required)?;
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
fn prompt_for_composite_param(
	cli: &mut impl Cli,
	param: &Param,
	force_required: bool,
) -> Result<String> {
	let mut field_values = Vec::new();
	for field_arg in &param.sub_params {
		let field_value = prompt_for_param(cli, field_arg, force_required)?;
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
fn prompt_for_tuple_param(
	cli: &mut impl Cli,
	param: &Param,
	force_required: bool,
) -> Result<String> {
	let mut tuple_values = Vec::new();
	for tuple_param in param.sub_params.iter() {
		let tuple_value = prompt_for_param(cli, tuple_param, force_required)?;
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use pop_chains::Function;
	use tempfile::tempdir;

	#[test]
	fn reset_for_new_call_works() -> Result<()> {
		let mut call_config = CallChainCommand {
			pallet: Some("System".to_string()),
			function: Some("remark".to_string()),
			args: vec!["0x11".to_string()],
			url: Some(Url::parse(urls::LOCAL)?),
			use_wallet: true,
			suri: Some(DEFAULT_URI.to_string()),
			skip_confirm: false,
			call_data: None,
			sudo: true,
			metadata: false,
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
	fn expand_file_arguments_works() -> Result<()> {
		let mut call_config = CallChainCommand {
			pallet: Some("Registrar".to_string()),
			function: Some("register".to_string()),
			args: vec!["2000".to_string(), "0x1".to_string(), "0x12".to_string()],
			url: Some(Url::parse(urls::LOCAL)?),
			suri: Some(DEFAULT_URI.to_string()),
			use_wallet: false,
			call_data: None,
			skip_confirm: false,
			sudo: false,
			metadata: false,
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
	fn resolve_function_args_preserves_cli_values() -> Result<()> {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			params: vec![Param { name: "remark".to_string(), ..Default::default() }],
			is_supported: true,
			..Default::default()
		};
		let mut call_config =
			CallChainCommand { args: vec!["0x11".to_string()], ..Default::default() };
		let mut cli = MockCli::new();
		let resolved = call_config.resolve_function_args(&function, &mut cli)?;
		assert_eq!(resolved, vec!["0x11".to_string()]);
		cli.verify()
	}

	#[test]
	fn resolve_function_args_prompts_for_missing_values() -> Result<()> {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			params: vec![
				Param { name: "first".to_string(), ..Default::default() },
				Param { name: "second".to_string(), ..Default::default() },
			],
			is_supported: true,
			..Default::default()
		};
		let mut call_config =
			CallChainCommand { args: vec!["0x11".to_string()], ..Default::default() };
		let mut cli =
			MockCli::new().expect_input("Enter the value for the parameter: second", "0x22".into());
		let resolved = call_config.resolve_function_args(&function, &mut cli)?;
		assert_eq!(resolved, vec!["0x11".to_string(), "0x22".to_string()]);
		assert_eq!(call_config.args, resolved);
		cli.verify()
	}

	#[test]
	fn parse_pallet_name_works() -> Result<()> {
		assert_eq!(parse_pallet_name("system").unwrap(), "System");
		assert_eq!(parse_pallet_name("balances").unwrap(), "Balances");
		assert_eq!(parse_pallet_name("nfts").unwrap(), "Nfts");
		Ok(())
	}

	#[test]
	fn list_pallets_works() {
		let pallets = vec![
			Pallet {
				name: "System".to_string(),
				index: 0,
				docs: "System pallet for runtime".to_string(),
				functions: vec![],
				constants: vec![],
				state: vec![],
			},
			Pallet {
				name: "Balances".to_string(),
				index: 1,
				docs: "".to_string(), // No docs
				functions: vec![],
				constants: vec![],
				state: vec![],
			},
			Pallet {
				name: "Assets".to_string(),
				index: 2,
				docs: "Assets management".to_string(),
				functions: vec![],
				constants: vec![],
				state: vec![],
			},
		];

		let mut cli = MockCli::new()
			.expect_info("Available pallets (3):\n")
			.expect_plain("  System - System pallet for runtime")
			.expect_plain("  Balances")
			.expect_plain("  Assets - Assets management");

		assert!(list_pallets(&pallets, &mut cli).is_ok());
		assert!(cli.verify().is_ok());
	}

	#[test]
	fn determine_signing_method_works() -> Result<()> {
		let mut cli = MockCli::new();
		let mut cmd = CallChainCommand { suri: Some("//Alice".to_string()), ..Default::default() };
		let (use_wallet, suri) = cmd.determine_signing_method(&mut cli)?;
		assert!(!use_wallet);
		assert_eq!(suri, "//Alice");
		cmd = CallChainCommand { use_wallet: true, ..Default::default() };
		let (use_wallet, suri) = cmd.determine_signing_method(&mut cli)?;
		assert!(use_wallet);
		assert_eq!(suri, DEFAULT_URI);
		// Test skip_confirm and no signer bails
		cmd = CallChainCommand { skip_confirm: true, ..Default::default() };
		let res = cmd.determine_signing_method(&mut cli);
		assert!(res.is_err());
		assert_eq!(
			res.unwrap_err().to_string(),
			"When skipping confirmation, a signer must be provided via --use-wallet or --suri."
		);
		cmd = CallChainCommand { skip_confirm: true, use_wallet: true, ..Default::default() };
		let (use_wallet, suri) = cmd.determine_signing_method(&mut cli)?;
		assert!(use_wallet);
		assert_eq!(suri, DEFAULT_URI);
		cli.verify()?;
		Ok(())
	}
}
