// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::{Cli, Confirm, Input, Select},
	common::{
		builds::{ensure_project_path, get_project_path},
		contracts::{
			build_contract_artifacts, has_contract_been_built, map_account, normalize_call_args,
			resolve_function_args, resolve_signer,
		},
		prompt::display_message,
		rpc::prompt_to_select_chain_rpc,
		urls,
		wallet::request_signature,
	},
};
use anyhow::{Result, anyhow};
use clap::Args;
use pop_common::{DefaultConfig, Keypair, parse_h160_account};
use pop_contracts::{
	CallExec, CallOpts, ContractCallable, ContractFunction, ContractStorage, DefaultEnvironment,
	Verbosity, Weight, call_smart_contract, call_smart_contract_from_signed_payload,
	dry_run_gas_estimate_call, fetch_contract_storage_with_param, get_call_payload,
	get_contract_storage_info, get_messages, set_up_call,
};
use serde::Serialize;
use std::path::PathBuf;

const DEFAULT_URI: &str = "//Alice";
const DEFAULT_PAYABLE_VALUE: &str = "0";

#[derive(Args, Clone, Serialize)]
pub struct CallContractCommand {
	/// Path to the contract build directory or a contract artifact.
	#[serde(skip_serializing)]
	#[arg(short, long)]
	path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[serde(skip_serializing)]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	/// The address of the contract to call.
	#[arg(short, long, env = "CONTRACT")]
	pub(crate) contract: Option<String>,
	/// The name of the contract message to call.
	#[arg(short, long)]
	message: Option<String>,
	/// The message arguments, encoded as strings.
	#[arg(short, long, num_args = 0..,)]
	args: Vec<String>,
	/// The value to be transferred as part of the call.
	#[arg(short, long, default_value = DEFAULT_PAYABLE_VALUE)]
	value: String,
	/// Maximum amount of gas to be used for this command.
	/// If not specified it will perform a dry-run to estimate the gas consumed for the
	/// call.
	#[arg(name = "gas", short, long, requires = "proof_size")]
	gas_limit: Option<u64>,
	/// Maximum proof size for this command.
	/// If not specified it will perform a dry-run to estimate the proof size required.
	#[arg(short = 'P', long, requires = "gas")]
	proof_size: Option<u64>,
	/// Websocket endpoint of a node.
	#[arg(short, long, value_parser)]
	pub(crate) url: Option<url::Url>,
	/// Secret key URI for the account calling the contract.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[serde(skip_serializing)]
	#[arg(short, long)]
	pub(crate) suri: Option<String>,
	/// Use a browser extension wallet to sign the extrinsic.
	#[arg(
		name = "use-wallet",
		long,
		short = 'w',
		default_value = "false",
		conflicts_with = "suri"
	)]
	pub(crate) use_wallet: bool,
	/// Submit an extrinsic for on-chain execution.
	#[arg(short = 'x', long)]
	pub(crate) execute: bool,
	/// Whether the contract was just deployed or not.
	#[arg(hide = true, long, default_value = "false")]
	pub(crate) deployed: bool,
	/// Automatically submits the call without prompting for confirmation.
	#[arg(short = 'y', long)]
	pub(crate) skip_confirm: bool,
	/// Optional key to query in case the selected storage is a mapping.
	#[arg(short = 'k', long)]
	storage_mapping_key: Option<String>,
}

impl Default for CallContractCommand {
	fn default() -> Self {
		Self {
			path: None,
			path_pos: None,
			contract: None,
			message: None,
			args: vec![],
			value: DEFAULT_PAYABLE_VALUE.to_string(),
			gas_limit: None,
			proof_size: None,
			url: None,
			suri: Some("//Alice".to_string()),
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		}
	}
}

impl CallContractCommand {
	fn url(&self) -> Result<url::Url> {
		self.url.as_ref().ok_or(anyhow::anyhow!("url not set")).cloned()
	}

	/// Executes the command.
	pub(crate) async fn execute(mut self, cli: &mut impl Cli) -> Result<()> {
		// Check if message specified via command line argument.
		let prompt_to_repeat_call = self.message.is_none();
		// Configure the call based on command line arguments/call UI.
		let callable = match self.configure(cli, false).await {
			Ok(c) => c,
			Err(e) => {
				match e.to_string().as_str() {
					"Contract not deployed." => {
						display_message(
							"Use `pop up` to deploy your contract.",
							true, // Not an error, just a message.
							cli,
						)?;
						return Ok(());
					},
					_ => {
						display_message(&e.to_string(), false, cli)?;
						return Err(e);
					},
				}
			},
		};
		// Finally execute the call.
		self.execute_call(cli, prompt_to_repeat_call, callable).await?;
		Ok(())
	}

	fn display(&self) -> String {
		let mut full_message = "pop call contract".to_string();

		if let Some(path) = &self.path {
			full_message.push_str(&format!(" --path {}", path.display()));
		}
		if let Some(path_pos) = &self.path_pos {
			full_message.push_str(&format!(" --path {}", path_pos.display()));
		}
		if let Some(contract) = &self.contract {
			full_message.push_str(&format!(" --contract {}", contract));
		}
		if let Some(message) = &self.message {
			full_message.push_str(&format!(" --message {}", message));
		}
		if !self.args.is_empty() {
			let args: Vec<_> = self.args.iter().map(|a| format!("\"{a}\"")).collect();
			full_message.push_str(&format!(" --args {}", args.join(" ")));
		}
		if let Some(storage_mapping_key) = &self.storage_mapping_key {
			full_message.push_str(&format!(" --storage-mapping-key {}", storage_mapping_key));
		}
		if self.value != DEFAULT_PAYABLE_VALUE {
			full_message.push_str(&format!(" --value {}", self.value));
		}
		if let (Some(gas_limit), Some(proof_size)) = (self.gas_limit, self.proof_size) {
			full_message.push_str(&format!(" --gas {} --proof-size {}", gas_limit, proof_size));
		}
		if let Some(url) = &self.url {
			full_message.push_str(&format!(" --url {}", url));
		}
		if self.use_wallet {
			full_message.push_str(" --use-wallet");
		} else if let Some(suri) = &self.suri {
			full_message.push_str(&format!(" --suri {}", suri));
		}
		if self.execute {
			full_message.push_str(" --execute");
		}
		if self.skip_confirm {
			full_message.push_str(" --skip-confirm");
		}
		full_message
	}

	/// If the contract has not been built, build it in release mode.
	async fn ensure_contract_built(&self, cli: &mut impl Cli) -> Result<()> {
		let project_path = ensure_project_path(self.path.clone(), self.path_pos.clone());
		cli.warning("NOTE: contract has not yet been built.")?;
		build_contract_artifacts(cli, &project_path, true, Verbosity::Quiet, None)
	}
	/// Prompts the user to confirm if the contract has already been deployed.
	fn confirm_contract_deployment(&self, cli: &mut impl Cli) -> Result<()> {
		let is_contract_deployed = cli
			.confirm("Has the contract already been deployed?")
			.initial_value(false)
			.interact()?;
		if !is_contract_deployed {
			return Err(anyhow!("Contract not deployed."));
		}
		Ok(())
	}

	fn should_confirm_contract_deployment(&self) -> bool {
		self.contract.is_none() && !self.deployed && !self.skip_confirm
	}

	/// Checks whether building the contract is required
	fn is_contract_build_required(&self) -> bool {
		let project_path = get_project_path(self.path.clone(), self.path_pos.clone());

		project_path
			.as_ref()
			.map(|p| p.is_dir() && !has_contract_been_built(p))
			.unwrap_or_default()
	}

	fn configure_storage(&mut self, cli: &mut impl Cli, storage: &ContractStorage) -> Result<()> {
		// Display storage field information
		self.use_wallet = false;
		self.suri = None;
		if self.storage_mapping_key.is_none() {
			self.storage_mapping_key = if let Some(key_type_name) = &storage.key_type_name &&
				!self.skip_confirm
			{
				let key: String = cli
					.input("Provide the mapping key to query (leave blank to fetch all)")
					.placeholder(key_type_name)
					.default_input("")
					.interact()?;
				if key.trim().is_empty() { None } else { Some(key) }
			} else {
				None
			};
		}
		Ok(())
	}

	fn configure_message(&mut self, message: &ContractFunction, cli: &mut impl Cli) -> Result<()> {
		resolve_function_args(message, cli, &mut self.args, self.skip_confirm)?;

		// Resolve value.
		if message.payable && self.value == DEFAULT_PAYABLE_VALUE {
			self.value = cli
				.input("Value to transfer to the call:")
				.placeholder("0")
				.default_input("0")
				.validate(|input: &String| match input.parse::<u64>() {
					Ok(_) => Ok(()),
					Err(_) => Err("Invalid value."),
				})
				.interact()?;
		}

		// Resolve who is calling the contract. If a `suri` was provided via the command line, skip
		// the prompt. Only prompt for mutations since read-only operations don't require signing.
		if message.mutates {
			resolve_signer(self.skip_confirm, &mut self.use_wallet, &mut self.suri, cli)?;
		}

		// Finally prompt for confirmation.
		if message.mutates {
			if !self.skip_confirm && !self.use_wallet && !self.execute {
				self.execute = cli
					.confirm(
						"Do you want to execute the call? (Selecting 'No' will perform a dry run)",
					)
					.initial_value(true)
					.interact()?;
			} else {
				self.execute = true;
			}
		} else {
			self.execute = false;
		}
		Ok(())
	}

	/// Configure the call based on command line arguments/call UI.
	async fn configure(&mut self, cli: &mut impl Cli, repeat: bool) -> Result<ContractCallable> {
		let mut project_path = get_project_path(self.path.clone(), self.path_pos.clone());

		// Show intro on first run.
		if !repeat {
			cli.intro("Call a contract")?;
		}

		// Resolve path.
		if project_path.is_none() {
			let current_dir = std::env::current_dir()?;
			let path = if matches!(pop_contracts::is_supported(&current_dir), Ok(true)) {
				current_dir
			} else {
				let input_path: String = cli
					.input("Where is your project or contract artifact located?")
					.placeholder("./")
					.default_input("./")
					.interact()?;
				PathBuf::from(input_path)
			};
			project_path = Some(path);
			self.path = project_path.clone();
		}
		let contract_path = project_path
			.as_ref()
			.expect("path is guaranteed to be set as input as prompted when None; qed");

		// Ensure contract is built and check if deployed.
		if self.is_contract_build_required() {
			self.ensure_contract_built(cli).await?;
			if self.should_confirm_contract_deployment() {
				self.confirm_contract_deployment(cli)?;
			}
		}

		// Parse the contract metadata provided. If there is an error, do not prompt for more.
		let messages = match get_messages(contract_path) {
			Ok(messages) => messages,
			Err(e) => {
				return Err(anyhow!(format!(
					"Unable to fetch contract metadata: {}",
					e.to_string().replace("Anyhow error: ", "")
				)));
			},
		};
		let storage = get_contract_storage_info(contract_path).unwrap_or_default();
		let mut callables = Vec::new();
		messages
			.into_iter()
			.for_each(|message| callables.push(ContractCallable::Function(message)));
		storage
			.into_iter()
			.for_each(|storage| callables.push(ContractCallable::Storage(storage)));

		// Resolve url.
		if !repeat && !self.deployed && self.url.is_none() {
			self.url = Some(
				prompt_to_select_chain_rpc(
					"Where is your contract deployed? (type to filter)",
					"Type the chain URL manually",
					urls::LOCAL,
					|n| n.supports_contracts,
					cli,
				)
				.await?,
			);
		};

		// Resolve contract address.
		if self.contract.is_none() {
			// Prompt for contract address.
			let contract_address: String = cli
				.input("Provide the on-chain contract address:")
				.placeholder("e.g. 0x48550a4bb374727186c55365b7c9c0a1a31bdafe")
				.required(true)
				.validate(|input: &String| match parse_h160_account(input) {
					Ok(_) => Ok(()),
					Err(_) => Err("Invalid address."),
				})
				.interact()?;
			self.contract = Some(contract_address);
		};

		// Resolve message.
		let callable = if let Some(ref message_name) = self.message {
			callables
				.iter()
				.find(|c| c.name() == message_name.as_str())
				.cloned()
				.ok_or_else(|| {
					anyhow::anyhow!(
						"Message '{}' not found in contract '{}'",
						message_name,
						contract_path.display()
					)
				})?
		} else {
			// No message provided, prompt user to select one
			let mut prompt = cli.select("Select the message to call (type to filter)");
			for callable in &callables {
				prompt = prompt.item(callable, callable.hint(), callable.docs());
			}
			let callable = prompt.filter_mode().interact()?;
			self.message = Some(callable.name());
			callable.clone()
		};

		match &callable {
			ContractCallable::Function(f) => self.configure_message(f, cli)?,
			ContractCallable::Storage(s) => self.configure_storage(cli, s)?,
		}

		cli.info(self.display())?;
		Ok(callable.clone())
	}

	async fn read_storage(&mut self, cli: &mut impl Cli, storage: ContractStorage) -> Result<()> {
		let value = fetch_contract_storage_with_param(
			&storage,
			self.contract.as_ref().expect("no contract address specified"),
			&self.url()?,
			&ensure_project_path(self.path.clone(), self.path_pos.clone()),
			self.storage_mapping_key.as_deref(),
		)
		.await?;
		cli.success(value)?;
		Ok(())
	}

	#[allow(deprecated)]
	async fn execute_message(
		&mut self,
		cli: &mut impl Cli,
		message: ContractFunction,
	) -> Result<()> {
		let project_path = ensure_project_path(self.path.clone(), self.path_pos.clone());
		// Disable wallet signing and display warning if the call is read-only.
		if !message.mutates && self.use_wallet {
			cli.warning("NOTE: Signing is not required for this read-only call. The '--use-wallet' flag will be ignored.")?;
			self.use_wallet = false;
		}

		let contract = match &self.contract {
			Some(contract) => contract.to_string(),
			None => {
				return Err(anyhow!("Please specify the contract address."));
			},
		};
		normalize_call_args(&mut self.args, &message);
		let (gas_limit, proof_size) =
			if let (Some(gas_limit), Some(proof_size)) = (self.gas_limit, self.proof_size) {
				(Some(gas_limit), Some(proof_size))
			} else {
				(None, None)
			};
		let call_exec = match set_up_call(CallOpts {
			path: project_path,
			contract,
			message: message.label,
			args: self.args.clone(),
			value: self.value.clone(),
			gas_limit,
			proof_size,
			url: self.url()?,
			suri: self.suri.clone().unwrap_or(DEFAULT_URI.to_string()),
			execute: self.execute,
		})
		.await
		{
			Ok(call_exec) => call_exec,
			Err(e) => {
				return Err(anyhow!(format!("{e}")));
			},
		};

		// Perform signing steps with wallet integration, skipping secure signing for query-only
		// operations.
		if self.use_wallet {
			self.execute_with_wallet(call_exec, cli).await?;
			return Ok(());
		}

		// Check if the account is already mapped, and prompt the user to perform the mapping if
		// it's required.
		map_account(call_exec.opts(), cli).await?;

		let spinner = cli.spinner();
		spinner.start("Doing a dry run...");
		let (call_dry_run_result, estimated_weight) =
			match dry_run_gas_estimate_call(&call_exec).await {
				Ok(w) => w,
				Err(e) => {
					spinner.error(format!("{e}"));
					display_message("Call failed.", false, cli)?;
					return Ok(());
				},
			};

		if self.execute {
			let weight_limit =
				if let (Some(gas_limit), Some(proof_size)) = (self.gas_limit, self.proof_size) {
					Weight::from_parts(gas_limit, proof_size)
				} else {
					estimated_weight
				};

			spinner.set_message("Calling the contract...");
			let call_result = call_smart_contract(call_exec, weight_limit, &self.url()?)
				.await
				.map_err(|err| anyhow!("ERROR: {err:?}"))?;
			spinner.clear();
			cli.info(call_result)?;
		} else {
			cli.success(call_dry_run_result)?;
			cli.warning("Your call has not been executed.")?;
		}

		Ok(())
	}

	/// Execute the function call or storage read.
	async fn execute_call(
		&mut self,
		cli: &mut impl Cli,
		prompt_to_repeat_call: bool,
		callable: ContractCallable,
	) -> Result<()> {
		match callable {
			ContractCallable::Function(f) => self.execute_message(cli, f).await,
			ContractCallable::Storage(s) => self.read_storage(cli, s).await,
		}?;
		self.finalize_execute_call(cli, prompt_to_repeat_call).await
	}

	/// Finalize the current call, prompting the user to repeat or conclude the process.
	async fn finalize_execute_call(
		&mut self,
		cli: &mut impl Cli,
		prompt_to_repeat_call: bool,
	) -> Result<()> {
		// Prompt for any additional calls.
		if !prompt_to_repeat_call {
			display_message("Call completed successfully!", true, cli)?;
			return Ok(());
		}
		if cli
			.confirm("Do you want to perform another call using the existing smart contract?")
			.initial_value(true)
			.interact()?
		{
			// Reset specific items from the last call and repeat.
			let mut new_call = self.clone();
			new_call.reset_for_new_call();
			Box::pin(new_call.execute(cli)).await
		} else {
			display_message("Contract calling complete.", true, cli)?;
			Ok(())
		}
	}

	/// Execute the smart contract call using wallet integration.
	async fn execute_with_wallet(
		&self,
		call_exec: CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
		cli: &mut impl Cli,
	) -> Result<()> {
		// Skip storage deposit estimation when using wallet (UI will handle it)
		let storage_deposit_limit = call_exec.opts().storage_deposit_limit().unwrap_or(0);
		let call_data = self
			.get_contract_data(&call_exec, storage_deposit_limit)
			.map_err(|err| anyhow!("An error occurred getting the call data: {err}"))?;

		let maybe_payload = request_signature(cli, call_data, self.url()?.to_string()).await?;
		if let Some(payload) = maybe_payload {
			cli.success("Signed payload received.")?;
			let spinner = cli.spinner();
			spinner
				.start("Calling the contract and waiting for finalization, please be patient...");

			let call_result =
				call_smart_contract_from_signed_payload(call_exec, payload, &self.url()?)
					.await
					.map_err(|err| anyhow!("ERROR: {err:?}"))?;

			cli.info(call_result)?;
		} else {
			display_message("No signed payload received.", false, cli)?;
		}
		Ok(())
	}

	// Get the call data.
	#[allow(deprecated)]
	fn get_contract_data(
		&self,
		call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
		storage_deposit_limit: u128,
	) -> anyhow::Result<Vec<u8>> {
		let weight_limit =
			if let (Some(gas_limit), Some(proof_size)) = (self.gas_limit, self.proof_size) {
				Weight::from_parts(gas_limit, proof_size)
			} else {
				Weight::zero()
			};
		let call_data = get_call_payload(call_exec, weight_limit, storage_deposit_limit)?;
		Ok(call_data)
	}

	/// Resets message specific fields to default values for a new call.
	#[allow(deprecated)]
	fn reset_for_new_call(&mut self) {
		self.message = None;
		self.value = DEFAULT_PAYABLE_VALUE.to_string();
		self.gas_limit = None;
		self.proof_size = None;
		self.use_wallet = false;
		self.execute = false;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		cli::MockCli,
		common::{urls, wallet::USE_WALLET_PROMPT},
	};
	use pop_contracts::{Param, mock_build_process, new_environment};
	use std::{env, fs::write};
	use url::Url;

	const CONTRACT_FILE: &str = "pop-contracts/tests/files/testing.contract";

	// This test only covers the interactive portion of the call contract command, without actually
	// calling the contract.
	#[tokio::test]
	#[allow(deprecated)]
	async fn guide_user_to_query_contract_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let items = vec![
            ("📝 [MUTATES] flip".into(), "A message that can be called on instantiated contracts. This one flips the value of the stored `bool` from `true` to `false` and vice versa.".into()),
            ("[READS] get".into(), "Simply returns the current value of our `bool`.".into()),
            ("📝 [MUTATES] specific_flip".into(), "A message for testing, flips the value of the stored `bool` with `new_value` and is payable".into()),
            ("[STORAGE] number".into(), "u32".into()),
            ("[STORAGE] value".into(), "bool".into()),
        ];
		// The inputs are processed in reverse order.
		let mut cli = MockCli::new()
			.expect_input("Provide the on-chain contract address:", "0x48550a4bb374727186c55365b7c9c0a1a31bdafe".into())
			.expect_select(
				"Select the message to call (type to filter)",
				Some(false),
				true,
				Some(items),
				1, // "get" message
				None,
			)
			.expect_info(format!(
				"pop call contract --path {} --contract 0x48550a4bb374727186c55365b7c9c0a1a31bdafe --message get --url {}",
				temp_dir.path().join("testing").display(),
				urls::LOCAL
			));

		let mut call_config = CallContractCommand {
			path: None,
			path_pos: Some(temp_dir.path().join("testing")),
			contract: None,
			message: None,
			args: vec![],
			value: DEFAULT_PAYABLE_VALUE.to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: None,
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		};
		call_config.configure(&mut cli, false).await?;
		assert_eq!(
			call_config.contract,
			Some("0x48550a4bb374727186c55365b7c9c0a1a31bdafe".to_string())
		);
		assert_eq!(call_config.message, Some("get".to_string()));
		assert_eq!(call_config.args.len(), 0);
		assert_eq!(call_config.value, "0".to_string());
		assert_eq!(call_config.gas_limit, None);
		assert_eq!(call_config.proof_size, None);
		assert_eq!(call_config.url()?.to_string(), urls::LOCAL);
		assert_eq!(call_config.suri, None);
		assert!(!call_config.execute);
		assert_eq!(
			call_config.display(),
			format!(
				"pop call contract --path {} --contract 0x48550a4bb374727186c55365b7c9c0a1a31bdafe --message get --url {}",
				temp_dir.path().join("testing").display(),
				urls::LOCAL
			)
		);

		cli.verify()
	}

	#[test]
	fn configure_message_prompts_for_remaining_args() -> Result<()> {
		let message = ContractFunction {
			label: "run".into(),
			payable: false,
			args: vec![
				Param { label: "first".into(), type_name: "u32".into() },
				Param { label: "second".into(), type_name: "u32".into() },
			],
			docs: String::new(),
			default: false,
			mutates: true,
		};

		let mut command = CallContractCommand {
			args: vec!["10".to_string()],
			value: DEFAULT_PAYABLE_VALUE.to_string(),
			skip_confirm: false,
			..Default::default()
		};

		let mut cli =
			MockCli::new().expect_input("Enter the value for the parameter: second", "20".into());

		command.configure_message(&message, &mut cli)?;

		assert_eq!(command.args, vec!["10".to_string(), "20".to_string()]);
		cli.verify()
	}

	#[test]
	fn configure_message_does_not_prompt_execute_when_execute_flag_set() -> Result<()> {
		let message = ContractFunction {
			label: "run".into(),
			payable: false,
			args: vec![],
			docs: String::new(),
			default: false,
			mutates: true,
		};

		let mut command = CallContractCommand {
			execute: true,
			suri: Some("//Alice".to_string()),
			use_wallet: false,
			skip_confirm: false,
			..Default::default()
		};

		let mut cli = MockCli::new();
		command.configure_message(&message, &mut cli)?;
		assert!(command.execute);
		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually
	// calling the contract.
	#[tokio::test]
	#[allow(deprecated)]
	async fn guide_user_to_call_contract_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let items = vec![
            ("📝 [MUTATES] flip".into(), "A message that can be called on instantiated contracts. This one flips the value of the stored `bool` from `true` to `false` and vice versa.".into()),
            ("[READS] get".into(), "Simply returns the current value of our `bool`.".into()),
            ("📝 [MUTATES] specific_flip".into(), "A message for testing, flips the value of the stored `bool` with `new_value` and is payable".into()),
            ("[STORAGE] number".into(), "u32".into()),
            ("[STORAGE] value".into(), "bool".into()),
        ];
		// The inputs are processed in reverse order.
		let mut cli = MockCli::new()
            .expect_input(
                "Provide the on-chain contract address:",
                "0x48550a4bb374727186c55365b7c9c0a1a31bdafe".into(),
            )
            .expect_select(
                "Select the message to call (type to filter)",
                Some(false),
                true,
                Some(items),
                2, // "specific_flip" message
                None,
            )
            .expect_input("Enter the value for the parameter: new_value", "true".into()) // Args for specific_flip
            .expect_input("Enter the value for the parameter: number", "2".into()) // Args for specific_flip
            .expect_input("Value to transfer to the call:", "50".into()) // Only if payable
            .expect_confirm(USE_WALLET_PROMPT, true)
            .expect_info(format!(
                "pop call contract --path {} --contract 0x48550a4bb374727186c55365b7c9c0a1a31bdafe --message specific_flip --args \"true\" \"2\" --value 50 --url {} --use-wallet --execute",
                temp_dir.path().join("testing").display(), urls::LOCAL
            ));

		let mut call_config = CallContractCommand {
			path: None,
			path_pos: Some(temp_dir.path().join("testing")),
			contract: None,
			message: None,
			args: vec![],
			value: DEFAULT_PAYABLE_VALUE.to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: None,
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		};
		call_config.configure(&mut cli, false).await?;
		assert_eq!(
			call_config.contract,
			Some("0x48550a4bb374727186c55365b7c9c0a1a31bdafe".to_string())
		);
		assert_eq!(call_config.message, Some("specific_flip".to_string()));
		assert_eq!(call_config.args.len(), 2);
		assert_eq!(call_config.args[0], "true".to_string());
		assert_eq!(call_config.args[1], "2".to_string());
		assert_eq!(call_config.value, "50".to_string());
		assert_eq!(call_config.url()?.to_string(), urls::LOCAL);
		assert_eq!(call_config.suri, None);
		assert!(call_config.use_wallet);
		assert!(call_config.execute);
		assert_eq!(
			call_config.display(),
			format!(
				"pop call contract --path {} --contract 0x48550a4bb374727186c55365b7c9c0a1a31bdafe --message specific_flip --args \"true\" \"2\" --value 50 --url {} --use-wallet --execute",
				temp_dir.path().join("testing").display(),
				urls::LOCAL
			)
		);

		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually
	// calling the contract.
	#[tokio::test]
	#[allow(deprecated)]
	async fn guide_user_to_call_contract_with_skip_confirm_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let items = vec![
            ("📝 [MUTATES] flip".into(), "A message that can be called on instantiated contracts. This one flips the value of the stored `bool` from `true` to `false` and vice versa.".into()),
            ("[READS] get".into(), "Simply returns the current value of our `bool`.".into()),
            ("📝 [MUTATES] specific_flip".into(), "A message for testing, flips the value of the stored `bool` with `new_value` and is payable".into()),
            ("[STORAGE] number".into(), "u32".into()),
            ("[STORAGE] value".into(), "bool".into()),
        ];
		// The inputs are processed in reverse order.
		let mut cli = MockCli::new()
			.expect_input(
				"Provide the on-chain contract address:",
				"0x48550a4bb374727186c55365b7c9c0a1a31bdafe".into(),
			)
			.expect_select(
				"Select the message to call (type to filter)",
				Some(false),
				true,
				Some(items),
				2, // "specific_flip" message
				None,
			)
			.expect_input("Value to transfer to the call:", "50".into()) // Only if payable
			.expect_info(format!(
				"pop call contract --path {} --contract 0x48550a4bb374727186c55365b7c9c0a1a31bdafe --message specific_flip --args \"true\" \"2\" --value 50 --gas 100000 --proof-size 1000000 --url {} --suri //Alice --execute --skip-confirm",
				temp_dir.path().join("testing").display(), urls::LOCAL
			));

		let mut call_config = CallContractCommand {
			path: None,
			path_pos: Some(temp_dir.path().join("testing")),
			contract: None,
			message: None,
			args: vec!["true".to_string(), "2".to_string()],
			value: DEFAULT_PAYABLE_VALUE.to_string(),
			gas_limit: Some(100000),
			proof_size: Some(1000000),
			url: Some(Url::parse(urls::LOCAL)?),
			suri: Some("//Alice".to_string()),
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: true,
			storage_mapping_key: None,
		};
		call_config.configure(&mut cli, false).await?;
		assert_eq!(
			call_config.contract,
			Some("0x48550a4bb374727186c55365b7c9c0a1a31bdafe".to_string())
		);
		assert_eq!(call_config.message, Some("specific_flip".to_string()));
		assert_eq!(call_config.args.len(), 2);
		assert_eq!(call_config.args[0], "true".to_string());
		assert_eq!(call_config.args[1], "2".to_string());
		assert_eq!(call_config.value, "50".to_string());
		assert_eq!(call_config.gas_limit, Some(100000));
		assert_eq!(call_config.proof_size, Some(1000000));
		assert_eq!(call_config.url()?.to_string(), urls::LOCAL);
		assert_eq!(call_config.suri, Some("//Alice".to_string()));
		assert!(call_config.execute);
		assert!(call_config.skip_confirm);
		assert_eq!(
			call_config.display(),
			format!(
				"pop call contract --path {} --contract 0x48550a4bb374727186c55365b7c9c0a1a31bdafe --message specific_flip --args \"true\" \"2\" --value 50 --gas 100000 --proof-size 1000000 --url {} --suri //Alice --execute --skip-confirm",
				temp_dir.path().join("testing").display(),
				urls::LOCAL
			)
		);

		cli.verify()
	}

	#[tokio::test]
	#[allow(deprecated)]
	async fn guide_user_to_call_contract_fails_not_build() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		// Create invalid `.json`, `.contract` and binary files for testing
		let invalid_contract_path = temp_dir.path().join("testing.contract");
		let invalid_json_path = temp_dir.path().join("testing.json");
		let invalid_binary_path = temp_dir.path().join("testing.polkavm");
		write(&invalid_contract_path, b"This is an invalid contract file")?;
		write(&invalid_json_path, b"This is an invalid JSON file")?;
		write(&invalid_binary_path, b"This is an invalid WASM file")?;
		// Mock the build process to simulate a scenario where the contract is not properly built.
		mock_build_process(
			temp_dir.path().join("testing"),
			invalid_contract_path.clone(),
			invalid_contract_path.clone(),
		)?;
		// Test the path is a folder with an invalid build.
		let mut command = CallContractCommand {
			path: None,
			path_pos: Some(temp_dir.path().join("testing")),
			contract: None,
			message: None,
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: None,
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		};
		let mut cli = MockCli::new();
		assert!(
			matches!(command.configure(&mut cli, false).await, Err(message) if message.to_string().contains("Unable to fetch contract metadata"))
		);
		// Test the path is a file with invalid `.contract` file.
		command.path = Some(invalid_contract_path);
		assert!(
			matches!(command.configure(&mut cli, false).await, Err(message) if message.to_string().contains("Unable to fetch contract metadata"))
		);
		// Test the path is a file with invalid `.json` file.
		command.path = Some(invalid_json_path);
		assert!(
			matches!(command.configure(&mut cli, false).await, Err(message) if message.to_string().contains("Unable to fetch contract metadata"))
		);
		// Test the path is a file with invalid binary file.
		command.path = Some(invalid_binary_path);
		assert!(
			matches!(command.configure(&mut cli, false).await, Err(message) if message.to_string().contains("Unable to fetch contract metadata"))
		);
		cli.verify()
	}

	#[tokio::test]
	#[allow(deprecated)]
	async fn execute_contract_fails_no_message_or_contract() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		// Test case 1: No contract address specified
		// When there's no contract and no message, the user would be prompted interactively,
		// but without proper contract address, execute_message will fail with "Please specify the
		// contract address."
		let mut cli = MockCli::new()
			.expect_intro("Call a contract")
			.expect_input("Provide the on-chain contract address:", "invalid".into())
			.expect_outro_cancel("Invalid address.");

		let result = CallContractCommand {
			path: None,
			path_pos: Some(temp_dir.path().join("testing")),
			contract: None,
			message: Some("get".to_string()),
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: None,
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		}
		.execute(&mut cli)
		.await;

		assert!(result.is_err(), "should return error for invalid address");
		cli.verify()
	}

	#[tokio::test]
	#[allow(deprecated)]
	async fn confirm_contract_deployment_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let call_config = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: None,
			message: None,
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: None,
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		};
		// Contract is not deployed.
		let mut cli =
			MockCli::new().expect_confirm("Has the contract already been deployed?", false);
		assert!(
			matches!(call_config.confirm_contract_deployment(&mut cli), Err(message) if message.to_string() == "Contract not deployed.")
		);
		cli.verify()?;
		// Contract is deployed.
		cli = MockCli::new().expect_confirm("Has the contract already been deployed?", true);
		call_config.confirm_contract_deployment(&mut cli)?;
		cli.verify()
	}

	#[test]
	fn should_confirm_contract_deployment_works() {
		let command = CallContractCommand { contract: None, deployed: false, ..Default::default() };
		assert!(command.should_confirm_contract_deployment());

		let command = CallContractCommand {
			contract: Some("0x48550a4bb374727186c55365b7c9c0a1a31bdafe".to_string()),
			deployed: false,
			..Default::default()
		};
		assert!(!command.should_confirm_contract_deployment());

		let command = CallContractCommand { contract: None, deployed: true, ..Default::default() };
		assert!(!command.should_confirm_contract_deployment());

		let command = CallContractCommand {
			contract: None,
			deployed: false,
			skip_confirm: true,
			..Default::default()
		};
		assert!(!command.should_confirm_contract_deployment());
	}

	#[tokio::test]
	#[allow(deprecated)]
	async fn is_contract_build_required_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let call_config = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: None,
			message: None,
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: None,
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		};
		// Contract not build. Build is required.
		assert!(call_config.is_contract_build_required());
		// Mock build process. Build is not required.
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join(CONTRACT_FILE),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;
		assert!(!call_config.is_contract_build_required());
		Ok(())
	}

	#[tokio::test]
	#[allow(deprecated)]
	async fn execute_handles_generic_configure_error() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		// Create invalid contract files to trigger an error
		let invalid_contract_path = temp_dir.path().join("testing.contract");
		let invalid_json_path = temp_dir.path().join("testing.json");
		write(&invalid_contract_path, b"This is an invalid contract file")?;
		write(&invalid_json_path, b"This is an invalid JSON file")?;
		mock_build_process(
			temp_dir.path().join("testing"),
			invalid_contract_path.clone(),
			invalid_contract_path.clone(),
		)?;

		let command = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: None,
			message: None,
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: None,
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		};

		// We can't check the exact error message because it includes dynamic temp paths,
		// but we can verify that execute properly returns an error for hard failures.
		// The intro will be shown, then the error will be displayed via outro_cancel.
		let mut cli = MockCli::new().expect_intro("Call a contract");
		// Note: We skip checking the outro_cancel message since it contains dynamic paths

		// Execute should return an error for hard failures
		let result = command.execute(&mut cli).await;
		assert!(result.is_err(), "execute should return an error for hard failures");
		assert!(result.unwrap_err().to_string().contains("Unable to fetch contract metadata"));

		Ok(())
	}

	#[tokio::test]
	#[allow(deprecated)]
	async fn execute_handles_execute_call_error() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		// Command with no contract address, which will cause execute_call to fail
		let command = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: None,
			message: Some("get".to_string()),
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: None,
			use_wallet: false,
			execute: false,
			deployed: false,
			skip_confirm: false,
			storage_mapping_key: None,
		};

		let mut cli = MockCli::new()
			.expect_intro("Call a contract")
			.expect_input("Provide the on-chain contract address:", "".into())
			.expect_outro_cancel("Invalid address.");

		// Execute should return an error for validation failures
		let result = command.execute(&mut cli).await;
		assert!(result.is_err(), "execute should return an error for validation failures");
		cli.verify()
	}

	#[tokio::test]
	#[allow(deprecated)]
	async fn execute_sets_prompt_to_repeat_call_when_message_is_none() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let items = vec![
            ("📝 [MUTATES] flip".into(), "A message that can be called on instantiated contracts. This one flips the value of the stored `bool` from `true` to `false` and vice versa.".into()),
            ("[READS] get".into(), "Simply returns the current value of our `bool`.".into()),
            ("📝 [MUTATES] specific_flip".into(), "A message for testing, flips the value of the stored `bool` with `new_value` and is payable".into()),
            ("[STORAGE] number".into(), "u32".into()),
            ("[STORAGE] value".into(), "bool".into()),
        ];

		// Command with message = None, so prompt_to_repeat_call should be true
		let command = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: None,
			message: None, // This is None, so prompt_to_repeat_call will be true
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: Some("//Alice".to_string()),
			use_wallet: false,
			execute: false,
			deployed: true,
			skip_confirm: false,
			storage_mapping_key: None,
		};

		let mut cli = MockCli::new()
		.expect_intro("Call a contract")
		.expect_input("Provide the on-chain contract address:", "0x48550a4bb374727186c55365b7c9c0a1a31bdafe".into())
		.expect_select(
			"Select the message to call (type to filter)",
				Some(false),
				true,
				Some(items),
				1, // "get" message
				None,
			)
		.expect_info(format!(
			"pop call contract --path {} --contract 0x48550a4bb374727186c55365b7c9c0a1a31bdafe --message get --url {} --suri //Alice",
			temp_dir.path().join("testing").display(),
			urls::LOCAL
		));

		// Execute will fail when trying to connect to the node since there's no node running
		let result = command.execute(&mut cli).await;
		assert!(result.is_err(), "execute should fail when node is unavailable");
		cli.verify()
	}

	#[tokio::test]
	#[allow(deprecated)]
	async fn execute_sets_prompt_to_repeat_call_when_message_is_some() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		// Command with message = Some, so prompt_to_repeat_call should be false
		let command = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: Some("0x48550a4bb374727186c55365b7c9c0a1a31bdafe".to_string()),
			message: Some("get".to_string()), /* This is Some, so prompt_to_repeat_call will be
			                                   * false */
			args: vec![],
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Some(Url::parse(urls::LOCAL)?),
			suri: Some("//Alice".to_string()),
			use_wallet: false,
			execute: false,
			deployed: true,
			skip_confirm: false,
			storage_mapping_key: None,
		};

		let mut cli = MockCli::new().expect_intro("Call a contract").expect_info(format!(
		"pop call contract --path {} --contract 0x48550a4bb374727186c55365b7c9c0a1a31bdafe --message get --url {} --suri //Alice",
		temp_dir.path().join("testing").display(),
		urls::LOCAL
	));

		// Execute will fail when trying to connect to the node since there's no node running
		let result = command.execute(&mut cli).await;
		assert!(result.is_err(), "execute should fail when node is unavailable");
		cli.verify()
	}

	#[test]
	fn reset_for_new_call_resets_execute_flag() {
		let mut command = CallContractCommand {
			message: Some("get".into()),
			value: "10".into(),
			gas_limit: Some(1),
			proof_size: Some(2),
			use_wallet: true,
			execute: true,
			..Default::default()
		};

		command.reset_for_new_call();

		assert_eq!(command.message, None);
		assert_eq!(command.value, DEFAULT_PAYABLE_VALUE);
		assert_eq!(command.gas_limit, None);
		assert_eq!(command.proof_size, None);
		assert!(!command.use_wallet);
		assert!(!command.execute);
	}
}
