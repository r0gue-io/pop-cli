// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::*},
	common::{
		builds::get_project_path,
		contracts::has_contract_been_built,
		prompt::display_message,
		wallet::{prompt_to_use_wallet, request_signature},
	},
};
use anyhow::{anyhow, Result};
use clap::Args;
use cliclack::spinner;
#[cfg(feature = "wasm-contracts")]
use pop_common::parse_account;
use pop_common::{DefaultConfig, Keypair};
use pop_contracts::{
	build_smart_contract, call_smart_contract, call_smart_contract_from_signed_payload,
	dry_run_call, dry_run_gas_estimate_call, get_call_payload, get_message, get_messages,
	set_up_call, CallExec, CallOpts, DefaultEnvironment, Verbosity, Weight,
};
use std::path::PathBuf;
#[cfg(feature = "polkavm-contracts")]
use {crate::common::contracts::map_account, pop_common::parse_h160_account};

const DEFAULT_URL: &str = "ws://localhost:9944/";
const DEFAULT_URI: &str = "//Alice";
const DEFAULT_PAYABLE_VALUE: &str = "0";

#[derive(Args, Clone)]
pub struct CallContractCommand {
	/// Path to the contract build directory or a contract artifact.
	#[arg(short, long)]
	path: Option<PathBuf>,
	/// Directory path without flag for your project [default: current directory]
	#[arg(value_name = "PATH", index = 1, conflicts_with = "path")]
	pub(crate) path_pos: Option<PathBuf>,
	/// The address of the contract to call.
	#[arg(short, long, env = "CONTRACT")]
	contract: Option<String>,
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
	#[arg(name = "gas", short, long)]
	gas_limit: Option<u64>,
	/// Maximum proof size for this command.
	/// If not specified it will perform a dry-run to estimate the proof size required.
	#[arg(short = 'P', long)]
	proof_size: Option<u64>,
	/// Websocket endpoint of a node.
	#[arg(short, long, value_parser, default_value = DEFAULT_URL)]
	url: url::Url,
	/// Secret key URI for the account calling the contract.
	///
	/// e.g.
	/// - for a dev account "//Alice"
	/// - with a password "//Alice///SECRET_PASSWORD"
	#[arg(short, long, default_value = DEFAULT_URI)]
	suri: String,
	/// Use a browser extension wallet to sign the extrinsic.
	#[arg(
		name = "use-wallet",
		long,
		short = 'w',
		default_value = "false",
		conflicts_with = "suri"
	)]
	use_wallet: bool,
	/// Submit an extrinsic for on-chain execution.
	#[arg(short = 'x', long)]
	execute: bool,
	/// Perform a dry-run via RPC to estimate the gas usage. This does not submit a transaction.
	#[arg(short = 'D', long, conflicts_with = "execute")]
	dry_run: bool,
	/// Enables developer mode, bypassing certain user prompts for faster testing.
	/// Recommended for testing and local development only.
	#[arg(name = "dev", short, long, default_value = "false")]
	dev_mode: bool,
}

impl CallContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<()> {
		// Check if message specified via command line argument.
		let prompt_to_repeat_call = self.message.is_none();
		// Configure the call based on command line arguments/call UI.
		if let Err(e) = self.configure(&mut cli::Cli, false).await {
			match e.to_string().as_str() {
				"Contract not deployed." => {
					display_message(
						"Use `pop up contract` to deploy your contract.",
						true, // Not an error, just a message.
						&mut cli::Cli,
					)?;
				},
				_ => {
					display_message(&e.to_string(), false, &mut cli::Cli)?;
				},
			}
			return Ok(());
		};
		// Finally execute the call.
		if let Err(e) = self.execute_call(&mut cli::Cli, prompt_to_repeat_call).await {
			display_message(&e.to_string(), false, &mut cli::Cli)?;
		}
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
			full_message.push_str(&format!(" --args {}", args.join(", ")));
		}
		if self.value != DEFAULT_PAYABLE_VALUE {
			full_message.push_str(&format!(" --value {}", self.value));
		}
		if let Some(gas_limit) = self.gas_limit {
			full_message.push_str(&format!(" --gas {}", gas_limit));
		}
		if let Some(proof_size) = self.proof_size {
			full_message.push_str(&format!(" --proof-size {}", proof_size));
		}
		full_message.push_str(&format!(" --url {}", self.url));
		if self.use_wallet {
			full_message.push_str(" --use-wallet");
		} else {
			full_message.push_str(&format!(" --suri {}", self.suri));
		}
		if self.execute {
			full_message.push_str(" --execute");
		}
		if self.dry_run {
			full_message.push_str(" --dry-run");
		}
		full_message
	}

	/// If the contract has not been built, build it in release mode.
	async fn ensure_contract_built(&self, cli: &mut impl Cli) -> Result<()> {
		let project_path = get_project_path(self.path.clone(), self.path_pos.clone());
		// Build the contract in release mode
		cli.warning("NOTE: contract has not yet been built.")?;
		let spinner = spinner();
		spinner.start("Building contract in RELEASE mode...");
		let result = match build_smart_contract(project_path.as_deref(), true, Verbosity::Quiet) {
			Ok(result) => result,
			Err(e) => {
				return Err(anyhow!(format!(
                        "🚫 An error occurred building your contract: {}\nUse `pop build` to retry with build output.",
                        e.to_string()
                    )));
			},
		};
		spinner.stop(format!(
			"Your contract artifacts are ready. You can find them in: {}",
			result.target_directory.display()
		));
		Ok(())
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

	/// Checks whether building the contract is required
	fn is_contract_build_required(&self) -> bool {
		let project_path = get_project_path(self.path.clone(), self.path_pos.clone());

		project_path
			.as_ref()
			.map(|p| p.is_dir() && !has_contract_been_built(Some(p)))
			.unwrap_or_default()
	}

	/// Configure the call based on command line arguments/call UI.
	async fn configure(&mut self, cli: &mut impl Cli, repeat: bool) -> Result<()> {
		let mut project_path = get_project_path(self.path.clone(), self.path_pos.clone());

		// Show intro on first run.
		if !repeat {
			cli.intro("Call a contract")?;
		}

		// If message has been specified via command line arguments, return early.
		if self.message.is_some() {
			return Ok(());
		}

		// Resolve path.
		if project_path.is_none() {
			let input_path: String = cli
				.input("Where is your project or contract artifact located?")
				.placeholder("./")
				.default_input("./")
				.interact()?;
			project_path = Some(PathBuf::from(input_path));
		}
		let contract_path = project_path
			.as_ref()
			.expect("path is guaranteed to be set as input as prompted when None; qed");

		// Ensure contract is built and check if deployed.
		if self.is_contract_build_required() {
			self.ensure_contract_built(&mut cli::Cli).await?;
			self.confirm_contract_deployment(&mut cli::Cli)?;
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

		// Resolve url.
		if !repeat && self.url.as_str() == DEFAULT_URL {
			// Prompt for url.
			let url: String = cli
				.input("Where is your contract deployed?")
				.placeholder("ws://localhost:9944")
				.default_input("ws://localhost:9944")
				.interact()?;
			self.url = url::Url::parse(&url)?
		};

		// Resolve contract address.
		if self.contract.is_none() {
			// Prompt for contract address.
			let contract_address: String = cli
				.input("Provide the on-chain contract address:")
				.placeholder(
					#[cfg(feature = "wasm-contracts")]
					"e.g. 5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt",
					#[cfg(feature = "polkavm-contracts")]
					"e.g. 0x48550a4bb374727186c55365b7c9c0a1a31bdafe",
				)
				.validate(|input: &String| {
					#[cfg(feature = "wasm-contracts")]
					let account = parse_account(input);
					#[cfg(feature = "polkavm-contracts")]
					let account = parse_h160_account(input);
					match account {
						Ok(_) => Ok(()),
						Err(_) => Err("Invalid address."),
					}
				})
				.interact()?;
			self.contract = Some(contract_address);
		};

		// Resolve message.
		let message = {
			let mut prompt = cli.select("Select the message to call:");
			for select_message in &messages {
				prompt = prompt.item(
					select_message,
					format!("{}\n", &select_message.label),
					&select_message.docs,
				);
			}
			let message = prompt.interact()?;
			self.message = Some(message.label.clone());
			message
		};

		// Resolve message arguments.
		let mut contract_args = Vec::new();
		for arg in &message.args {
			let mut input = cli
				.input(format!("Enter the value for the parameter: {}", arg.label))
				.placeholder(&format!("Type required: {}", arg.type_name));

			// Set default input only if the parameter type is `Option` (Not mandatory)
			if arg.type_name.starts_with("Option<") {
				input = input.default_input("");
			}
			contract_args.push(input.interact()?);
		}
		self.args = contract_args;

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

		// Resolve gas limit.
		if message.mutates && !self.dev_mode && self.gas_limit.is_none() {
			// Prompt for gas limit and proof_size of the call.
			let gas_limit_input: String = cli
				.input("Enter the gas limit:")
				.required(false)
				.default_input("")
				.placeholder("If left blank, an estimation will be used")
				.interact()?;
			self.gas_limit = gas_limit_input.parse::<u64>().ok(); // If blank or bad input, estimate it.
		}

		// Resolve proof size.
		if message.mutates && !self.dev_mode && self.proof_size.is_none() {
			let proof_size_input: String = cli
				.input("Enter the proof size limit:")
				.required(false)
				.placeholder("If left blank, an estimation will be used")
				.default_input("")
				.interact()?;
			self.proof_size = proof_size_input.parse::<u64>().ok(); // If blank or bad input, estimate it.
		}

		// Resolve who is calling the contract. If a `suri` was provided via the command line, skip
		// the prompt.
		if self.suri == DEFAULT_URI && !self.use_wallet && message.mutates {
			if prompt_to_use_wallet(cli)? {
				self.use_wallet = true;
			} else {
				self.suri = cli
					.input("Signer calling the contract:")
					.placeholder("//Alice")
					.default_input("//Alice")
					.interact()?;
			};
		}

		// Finally prompt for confirmation.
		let is_call_confirmed = if message.mutates && !self.dev_mode && !self.use_wallet {
			cli.confirm("Do you want to execute the call? (Selecting 'No' will perform a dry run)")
				.initial_value(true)
				.interact()?
		} else {
			true
		};
		self.execute = is_call_confirmed && message.mutates;
		self.dry_run = !is_call_confirmed;

		cli.info(self.display())?;
		Ok(())
	}

	/// Execute the call.
	async fn execute_call(
		&mut self,
		cli: &mut impl Cli,
		prompt_to_repeat_call: bool,
	) -> Result<()> {
		let project_path = get_project_path(self.path.clone(), self.path_pos.clone());

		let message = match &self.message {
			Some(message) => message.to_string(),
			None => {
				return Err(anyhow!("Please specify the message to call."));
			},
		};
		// Disable wallet signing and display warning if the call is read-only.
		let path = PathBuf::from("./");
		let message_metadata =
			get_message(project_path.as_deref().unwrap_or_else(|| &path), &message)?;
		if !message_metadata.mutates && self.use_wallet {
			cli.warning("NOTE: Signing is not required for this read-only call. The '--use-wallet' flag will be ignored.")?;
			self.use_wallet = false;
		}

		let contract = match &self.contract {
			Some(contract) => contract.to_string(),
			None => {
				return Err(anyhow!("Please specify the contract address."));
			},
		};
		let call_exec = match set_up_call(CallOpts {
			path: project_path,
			contract,
			message,
			args: self.args.clone(),
			value: self.value.clone(),
			gas_limit: self.gas_limit,
			proof_size: self.proof_size,
			url: self.url.clone(),
			suri: self.suri.clone(),
			execute: self.execute,
		})
		.await
		{
			Ok(call_exec) => call_exec,
			Err(e) => {
				return Err(anyhow!(format!("{}", e.to_string())));
			},
		};
		// Check if the account is already mapped, and prompt the user to perform the mapping if
		// it's required.
		#[cfg(feature = "polkavm-contracts")]
		map_account(call_exec.opts(), cli).await?;

		// Perform signing steps with wallet integration, skipping secure signing for query-only
		// operations.
		if self.use_wallet {
			self.execute_with_wallet(call_exec, cli).await?;
			return self.finalize_execute_call(cli, prompt_to_repeat_call).await;
		}
		if self.dry_run {
			let spinner = spinner();
			spinner.start("Doing a dry run to estimate the gas...");
			match dry_run_gas_estimate_call(&call_exec).await {
				Ok(w) => {
					cli.info(format!("Gas limit: {:?}", w))?;
					cli.warning("Your call has not been executed.")?;
				},
				Err(e) => {
					spinner.error(format!("{e}"));
					display_message("Call failed.", false, cli)?;
				},
			};
			return Ok(());
		}

		if !self.execute {
			let spinner = spinner();
			spinner.start("Calling the contract...");
			let call_dry_run_result = dry_run_call(&call_exec).await?;
			spinner.stop("");
			cli.info(format!("Result: {}", call_dry_run_result))?;
			cli.warning("Your call has not been executed.")?;
		} else {
			let weight_limit = if self.gas_limit.is_some() && self.proof_size.is_some() {
				Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
			} else {
				let spinner = spinner();
				spinner.start("Doing a dry run to estimate the gas...");
				match dry_run_gas_estimate_call(&call_exec).await {
					Ok(w) => {
						cli.info(format!("Gas limit: {:?}", w))?;
						w
					},
					Err(e) => {
						spinner.error(format!("{e}"));
						return Err(anyhow!("Call failed."));
					},
				}
			};
			let spinner = spinner();
			spinner.start("Calling the contract...");

			let call_result = call_smart_contract(call_exec, weight_limit, &self.url)
				.await
				.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;

			cli.info(call_result)?;
		}
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
			.initial_value(false)
			.interact()?
		{
			// Reset specific items from the last call and repeat.
			self.reset_for_new_call();
			self.configure(cli, true).await?;
			Box::pin(self.execute_call(cli, prompt_to_repeat_call)).await
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
		#[cfg(feature = "polkavm-contracts")]
		let storage_deposit_limit = match call_exec.opts().storage_deposit_limit() {
			Some(deposit_limit) => deposit_limit,
			None => call_exec.estimate_gas().await?.1,
		};
		#[cfg(feature = "polkavm-contracts")]
		let call_data = self.get_contract_data(&call_exec, storage_deposit_limit).map_err(|err| {
			anyhow!("An error occurred getting the call data: {}", err.to_string())
		})?;
		#[cfg(feature = "wasm-contracts")]
		let call_data = self.get_contract_data(&call_exec).map_err(|err| {
			anyhow!("An error occurred getting the call data: {}", err.to_string())
		})?;

		let maybe_payload =
			request_signature(call_data, self.url.to_string()).await?.signed_payload;
		if let Some(payload) = maybe_payload {
			cli.success("Signed payload received.")?;
			let spinner = spinner();
			spinner
				.start("Calling the contract and waiting for finalization, please be patient...");

			let call_result =
				call_smart_contract_from_signed_payload(call_exec, payload, &self.url)
					.await
					.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;

			cli.info(call_result)?;
		} else {
			display_message("No signed payload received.", false, cli)?;
		}
		Ok(())
	}

	// Get the call data.
	fn get_contract_data(
		&self,
		call_exec: &CallExec<DefaultConfig, DefaultEnvironment, Keypair>,
		#[cfg(feature = "polkavm-contracts")] storage_deposit_limit: u128,
	) -> anyhow::Result<Vec<u8>> {
		let weight_limit = if self.gas_limit.is_some() && self.proof_size.is_some() {
			Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
		} else {
			Weight::zero()
		};
		#[cfg(feature = "wasm-contracts")]
		let call_data = get_call_payload(call_exec, weight_limit)?;
		#[cfg(feature = "polkavm-contracts")]
		let call_data = get_call_payload(call_exec, weight_limit, storage_deposit_limit)?;
		Ok(call_data)
	}

	/// Resets message specific fields to default values for a new call.
	fn reset_for_new_call(&mut self) {
		self.message = None;
		self.value = DEFAULT_PAYABLE_VALUE.to_string();
		self.gas_limit = None;
		self.proof_size = None;
		self.use_wallet = false;
	}
}

#[cfg(test)]
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
			url: url::Url::parse("wss://rpc1.paseo.popnetwork.xyz").unwrap(),
			suri: "//Alice".to_string(),
			use_wallet: false,
			execute: false,
			dry_run: false,
			dev_mode: false,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{cli::MockCli, common::wallet::USE_WALLET_PROMPT};
	use pop_contracts::{mock_build_process, new_environment};
	use std::{env, fs::write};
	use url::Url;

	#[cfg(feature = "wasm-contracts")]
	const CONTRACT_ADDRESS: &str = "15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm";
	#[cfg(feature = "polkavm-contracts")]
	const CONTRACT_ADDRESS: &str = "0x4f04054746fb19d3b027f5fe1ca5e87a68b49bac";
	#[cfg(feature = "wasm-contracts")]
	const CONTRACTS_NETWORK_URL: &str = "wss://rpc1.paseo.popnetwork.xyz/";
	#[cfg(feature = "polkavm-contracts")]
	const CONTRACTS_NETWORK_URL: &str = "wss://westend-asset-hub-rpc.polkadot.io/";

	#[tokio::test]
	async fn execute_query_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;
		// Contract deployed on Pop Network testnet, test get
		CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: Some(CONTRACT_ADDRESS.to_string()),
			message: Some("get".to_string()),
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			use_wallet: false,
			dry_run: false,
			execute: false,
			dev_mode: false,
		}
		.execute()
		.await?;
		Ok(())
	}

	#[tokio::test]
	async fn call_contract_dry_run_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let mut cli = MockCli::new()
			.expect_intro(&"Call a contract")
			.expect_warning("Your call has not been executed.")
			.expect_info("Gas limit: Weight { ref_time: 100, proof_size: 10 }");

		let mut call_config = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: Some(CONTRACT_ADDRESS.to_string()),
			message: Some("flip".to_string()),
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: Some(100),
			proof_size: Some(10),
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			use_wallet: false,
			dry_run: true,
			execute: false,
			dev_mode: false,
		};
		call_config.configure(&mut cli, false).await?;
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message flip --gas 100 --proof-size 10 --url {CONTRACTS_NETWORK_URL} --suri //Alice --dry-run",
			temp_dir.path().join("testing").display().to_string(),
		));
		// Contract deployed on Pop Network testnet, test dry-run
		call_config.execute_call(&mut cli, false).await?;

		cli.verify()
	}

	#[tokio::test]
	async fn call_contract_dry_run_with_artifact_file_works() -> Result<()> {
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();

		let mut cli = MockCli::new()
			.expect_intro(&"Call a contract")
			.expect_warning("Your call has not been executed.")
			.expect_info("Gas limit: Weight { ref_time: 100, proof_size: 10 }");

		// From .contract file
		let mut call_config = CallContractCommand {
			path: Some(current_dir.join("pop-contracts/tests/files/testing.contract")),
			path_pos: None,
			contract: Some(CONTRACT_ADDRESS.to_string()),
			message: Some("flip".to_string()),
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: Some(100),
			proof_size: Some(10),
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			use_wallet: false,
			dry_run: true,
			execute: false,
			dev_mode: false,
		};
		call_config.configure(&mut cli, false).await?;
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message flip --gas 100 --proof-size 10 --url {CONTRACTS_NETWORK_URL} --suri //Alice --dry-run",
			current_dir.join("pop-contracts/tests/files/testing.contract").display().to_string(),
		));
		// Contract deployed on Pop Network testnet, test dry-run
		call_config.execute_call(&mut cli, false).await?;

		// From .json file
		call_config.path = Some(current_dir.join("pop-contracts/tests/files/testing.json"));
		call_config.configure(&mut cli, false).await?;
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message flip --gas 100 --proof-size 10 --url {CONTRACTS_NETWORK_URL} --suri //Alice --dry-run",
			current_dir.join("pop-contracts/tests/files/testing.json").display().to_string(),
		));

		#[cfg(feature = "wasm-contracts")]
		let binary_path = "pop-contracts/tests/files/testing.wasm";
		#[cfg(feature = "polkavm-contracts")]
		let binary_path = "pop-contracts/tests/files/testing.polkavm";
		// From binary file
		call_config.path = Some(current_dir.join(binary_path));
		call_config.configure(&mut cli, false).await?;
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message flip --gas 100 --proof-size 10 --url {CONTRACTS_NETWORK_URL} --suri //Alice --dry-run",
			current_dir.join(binary_path).display().to_string(),
		));
		// Contract deployed on Pop Network testnet, test dry-run
		call_config.execute_call(&mut cli, false).await?;

		cli.verify()
	}

	#[tokio::test]
	#[cfg(feature = "wasm-contracts")]
	async fn call_contract_query_duplicate_call_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;
		let items = vec![
			("flip\n".into(), " A message that can be called on instantiated contracts.  This one flips the value of the stored `bool` from `true`  to `false` and vice versa.".into()),
			("get\n".into(), " Simply returns the current value of our `bool`.".into()),
			("specific_flip\n".into(), " A message for testing, flips the value of the stored `bool` with `new_value`  and is payable".into())
		];
		let mut cli = MockCli::new()
			.expect_intro(&"Call a contract")
			.expect_warning("Your call has not been executed.")
			.expect_confirm(
				"Do you want to perform another call using the existing smart contract?",
				true,
			)
			.expect_select(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				1, // "get" message
				None
			)
			.expect_info(format!(
			    "pop call contract --path {} --contract 15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm --message get --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Alice",
			    temp_dir.path().join("testing").display().to_string(),
			))
			.expect_warning("NOTE: Signing is not required for this read-only call. The '--use-wallet' flag will be ignored.")
			.expect_warning("Your call has not been executed.")
			.expect_confirm(
				"Do you want to perform another call using the existing smart contract?",
				false,
			)
			.expect_outro("Contract calling complete.");

		// Contract deployed on Pop Network testnet, test get
		let mut call_config = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: Some(CONTRACT_ADDRESS.to_string()),
			message: Some("get".to_string()),
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			use_wallet: true,
			dry_run: false,
			execute: false,
			dev_mode: false,
		};
		call_config.configure(&mut cli, false).await?;
		// Test the query. With true, it will prompt for another call.
		call_config.execute_call(&mut cli, true).await?;

		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually
	// calling the contract.
	#[tokio::test]
	async fn guide_user_to_query_contract_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let items = vec![
			("flip\n".into(), " A message that can be called on instantiated contracts.  This one flips the value of the stored `bool` from `true`  to `false` and vice versa.".into()),
			("get\n".into(), " Simply returns the current value of our `bool`.".into()),
			("specific_flip\n".into(), " A message for testing, flips the value of the stored `bool` with `new_value`  and is payable".into())
		];
		// The inputs are processed in reverse order.
		let mut cli = MockCli::new()
			.expect_select(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				1, // "get" message
				None
			)
			.expect_input(
				"Where is your contract deployed?",
				CONTRACTS_NETWORK_URL.into(),
			)
			.expect_input(
				"Provide the on-chain contract address:",
				CONTRACT_ADDRESS.into(),
			)
			.expect_info(format!(
	            "pop call contract --path {} --contract {CONTRACT_ADDRESS} --message get --url {CONTRACTS_NETWORK_URL} --suri //Alice",
	            temp_dir.path().join("testing").display().to_string(),
	        ));

		let mut call_config = CallContractCommand {
			path: None,
			path_pos: Some(temp_dir.path().join("testing")),
			contract: None,
			message: None,
			args: vec![].to_vec(),
			value: DEFAULT_PAYABLE_VALUE.to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(DEFAULT_URL)?,
			suri: DEFAULT_URI.to_string(),
			use_wallet: false,
			dry_run: false,
			execute: false,
			dev_mode: false,
		};
		call_config.configure(&mut cli, false).await?;
		assert_eq!(call_config.contract, Some(CONTRACT_ADDRESS.to_string()));
		assert_eq!(call_config.message, Some("get".to_string()));
		assert_eq!(call_config.args.len(), 0);
		assert_eq!(call_config.value, "0".to_string());
		assert_eq!(call_config.gas_limit, None);
		assert_eq!(call_config.proof_size, None);
		assert_eq!(call_config.url.to_string(), CONTRACTS_NETWORK_URL);
		assert_eq!(call_config.suri, "//Alice");
		assert!(!call_config.execute);
		assert!(!call_config.dry_run);
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message get --url {CONTRACTS_NETWORK_URL} --suri //Alice",
			temp_dir.path().join("testing").display().to_string()
		));

		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually
	// calling the contract.
	#[tokio::test]
	async fn guide_user_to_call_contract_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let items = vec![
			("flip\n".into(), " A message that can be called on instantiated contracts.  This one flips the value of the stored `bool` from `true`  to `false` and vice versa.".into()),
			("get\n".into(), " Simply returns the current value of our `bool`.".into()),
			("specific_flip\n".into(), " A message for testing, flips the value of the stored `bool` with `new_value`  and is payable".into())
		];
		// The inputs are processed in reverse order.
		let mut cli = MockCli::new()
			.expect_input(
				"Where is your contract deployed?",
				CONTRACTS_NETWORK_URL.into(),
			)
			.expect_input(
				"Provide the on-chain contract address:",
				CONTRACT_ADDRESS.into(),
			)
			.expect_select(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				2, // "specific_flip" message
				None
			)
			.expect_input("Enter the value for the parameter: new_value", "true".into()) // Args for specific_flip
			.expect_input("Enter the value for the parameter: number", "2".into()) // Args for specific_flip
			.expect_input("Value to transfer to the call:", "50".into()) // Only if payable
			.expect_input("Enter the gas limit:", "".into()) // Only if call
			.expect_input("Enter the proof size limit:", "".into()) // Only if call
			.expect_confirm(USE_WALLET_PROMPT, true)
			.expect_info(format!(
				"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message specific_flip --args \"true\", \"2\" --value 50 --url {CONTRACTS_NETWORK_URL} --use-wallet --execute",
				temp_dir.path().join("testing").display().to_string(),
			));

		let mut call_config = CallContractCommand {
			path: None,
			path_pos: Some(temp_dir.path().join("testing")),
			contract: None,
			message: None,
			args: vec![].to_vec(),
			value: DEFAULT_PAYABLE_VALUE.to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(DEFAULT_URL)?,
			suri: DEFAULT_URI.to_string(),
			use_wallet: false,
			dry_run: false,
			execute: false,
			dev_mode: false,
		};
		call_config.configure(&mut cli, false).await?;
		assert_eq!(call_config.contract, Some(CONTRACT_ADDRESS.to_string()));
		assert_eq!(call_config.message, Some("specific_flip".to_string()));
		assert_eq!(call_config.args.len(), 2);
		assert_eq!(call_config.args[0], "true".to_string());
		assert_eq!(call_config.args[1], "2".to_string());
		assert_eq!(call_config.value, "50".to_string());
		assert_eq!(call_config.gas_limit, None);
		assert_eq!(call_config.proof_size, None);
		assert_eq!(call_config.url.to_string(), CONTRACTS_NETWORK_URL);
		assert_eq!(call_config.suri, "//Alice");
		assert!(call_config.use_wallet);
		assert!(call_config.execute);
		assert!(!call_config.dry_run);
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message specific_flip --args \"true\", \"2\" --value 50 --url {CONTRACTS_NETWORK_URL} --use-wallet --execute",
			temp_dir.path().join("testing").display().to_string()
		));

		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually
	// calling the contract.
	#[tokio::test]
	async fn guide_user_to_call_contract_in_dev_mode_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let items = vec![
			("flip\n".into(), " A message that can be called on instantiated contracts.  This one flips the value of the stored `bool` from `true`  to `false` and vice versa.".into()),
			("get\n".into(), " Simply returns the current value of our `bool`.".into()),
			("specific_flip\n".into(), " A message for testing, flips the value of the stored `bool` with `new_value`  and is payable".into())
		];
		// The inputs are processed in reverse order.
		let mut cli = MockCli::new()
			.expect_select(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				2, // "specific_flip" message
				None
			)
			.expect_input(
				"Where is your contract deployed?",
				CONTRACTS_NETWORK_URL.into(),
			)
			.expect_input(
				"Provide the on-chain contract address:",
				CONTRACT_ADDRESS.into(),
			)
			.expect_input("Enter the value for the parameter: new_value", "true".into()) // Args for specific_flip
			.expect_input("Enter the value for the parameter: number", "2".into()) // Args for specific_flip
			.expect_input("Value to transfer to the call:", "50".into()) // Only if payable
			.expect_input("Signer calling the contract:", "//Alice".into())
			.expect_info(format!(
				"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message specific_flip --args \"true\", \"2\" --value 50 --url {CONTRACTS_NETWORK_URL} --suri //Alice --execute",
				temp_dir.path().join("testing").display().to_string(),
			));

		let mut call_config = CallContractCommand {
			path: None,
			path_pos: Some(temp_dir.path().join("testing")),
			contract: None,
			message: None,
			args: vec![].to_vec(),
			value: DEFAULT_PAYABLE_VALUE.to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(DEFAULT_URL)?,
			suri: DEFAULT_URI.to_string(),
			use_wallet: false,
			dry_run: false,
			execute: false,
			dev_mode: true,
		};
		call_config.configure(&mut cli, false).await?;
		assert_eq!(call_config.contract, Some(CONTRACT_ADDRESS.to_string()));
		assert_eq!(call_config.message, Some("specific_flip".to_string()));
		assert_eq!(call_config.args.len(), 2);
		assert_eq!(call_config.args[0], "true".to_string());
		assert_eq!(call_config.args[1], "2".to_string());
		assert_eq!(call_config.value, "50".to_string());
		assert_eq!(call_config.gas_limit, None);
		assert_eq!(call_config.proof_size, None);
		assert_eq!(call_config.url.to_string(), CONTRACTS_NETWORK_URL);
		assert_eq!(call_config.suri, "//Alice");
		assert!(call_config.execute);
		assert!(!call_config.dry_run);
		assert!(call_config.dev_mode);
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract {CONTRACT_ADDRESS} --message specific_flip --args \"true\", \"2\" --value 50 --url {CONTRACTS_NETWORK_URL} --suri //Alice --execute",
			temp_dir.path().join("testing").display().to_string()
		));

		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_call_contract_fails_not_build() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		// Create invalid `.json`, `.contract` and binary files for testing
		let invalid_contract_path = temp_dir.path().join("testing.contract");
		let invalid_json_path = temp_dir.path().join("testing.json");
		#[cfg(feature = "wasm-contracts")]
		let invalid_binary_path = temp_dir.path().join("testing.wasm");
		#[cfg(feature = "polkavm-contracts")]
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
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: None,
			message: None,
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			use_wallet: false,
			dry_run: false,
			execute: false,
			dev_mode: false,
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
	async fn execute_contract_fails_no_message_or_contract() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let mut cli = MockCli::new();
		assert!(matches!(
			CallContractCommand {
				path: Some(temp_dir.path().join("testing")),
				path_pos: None,
				contract: Some(CONTRACT_ADDRESS.to_string()),
				message: None,
				args: vec![].to_vec(),
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				url: Url::parse(CONTRACTS_NETWORK_URL)?,
				suri: "//Alice".to_string(),
				use_wallet: false,
				dry_run: false,
				execute: false,
				dev_mode: false,
			}.execute_call(&mut cli, false).await,
			anyhow::Result::Err(message) if message.to_string() == "Please specify the message to call."
		));

		assert!(matches!(
			CallContractCommand {
				path: Some(temp_dir.path().join("testing")),
				path_pos: None,
				contract: None,
				message: Some("get".to_string()),
				args: vec![].to_vec(),
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				url: Url::parse(CONTRACTS_NETWORK_URL)?,
				suri: "//Alice".to_string(),
				use_wallet: false,
				dry_run: false,
				execute: false,
				dev_mode: false,
			}.execute_call(&mut cli, false).await,
			anyhow::Result::Err(message) if message.to_string() == "Please specify the contract address."
		));

		cli.verify()
	}

	#[tokio::test]
	async fn confirm_contract_deployment_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let call_config = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: Some(CONTRACT_ADDRESS.to_string()),
			message: None,
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			use_wallet: false,
			dry_run: false,
			execute: false,
			dev_mode: false,
		};
		// Contract is not deployed.
		let mut cli =
			MockCli::new().expect_confirm("Has the contract already been deployed?", false);
		assert!(
			matches!(call_config.confirm_contract_deployment(&mut cli), anyhow::Result::Err(message) if message.to_string() == "Contract not deployed.")
		);
		cli.verify()?;
		// Contract is deployed.
		cli = MockCli::new().expect_confirm("Has the contract already been deployed?", true);
		call_config.confirm_contract_deployment(&mut cli)?;
		cli.verify()
	}

	#[tokio::test]
	async fn is_contract_build_required_works() -> Result<()> {
		let temp_dir = new_environment("testing")?;
		let call_config = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			path_pos: None,
			contract: Some(CONTRACT_ADDRESS.to_string()),
			message: None,
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse(CONTRACTS_NETWORK_URL)?,
			suri: "//Alice".to_string(),
			use_wallet: false,
			dry_run: false,
			execute: false,
			dev_mode: false,
		};
		// Contract not build. Build is required.
		assert!(call_config.is_contract_build_required());
		// Mock build process. Build is not required.
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;
		assert!(!call_config.is_contract_build_required());
		Ok(())
	}
}
