// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use anyhow::{anyhow, Result};
use clap::Args;
use pop_contracts::{
	call_smart_contract, dry_run_call, dry_run_gas_estimate_call, get_messages, parse_account,
	set_up_call, CallOpts,
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

pub(crate) struct CallContract<'a, CLI: Cli> {
	/// The cli to be used.
	pub(crate) cli: &'a mut CLI,
	/// The args to call.
	pub(crate) args: CallContractCommand,
}

impl<'a, CLI: Cli> CallContract<'a, CLI> {
	/// Executes the command.
	pub(crate) async fn execute(mut self: Box<Self>) -> Result<()> {
		self.cli.intro("Call a contract")?;

		let call_config = if self.args.contract.is_none() {
			guide_user_to_call_contract(&mut self).await?
		} else {
			self.args.clone()
		};
		let contract = call_config
			.contract
			.expect("contract can not be none as fallback above is interactive input; qed");
		// TODO: Can be nill pop call contract --contract <contract>
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
			url: call_config.url.clone(),
			suri: call_config.suri,
			execute: call_config.execute,
		})
		.await?;

		if call_config.dry_run {
			let spinner = cliclack::spinner();
			spinner.start("Doing a dry run to estimate the gas...");
			match dry_run_gas_estimate_call(&call_exec).await {
				Ok(w) => {
					self.cli.info(format!("Gas limit: {:?}", w))?;
					self.cli.warning("Your call has not been executed.")?;
				},
				Err(e) => {
					spinner.error(format!("{e}"));
					self.cli.outro_cancel("Call failed.")?;
				},
			};
			return Ok(());
		}

		if !call_config.execute {
			let spinner = cliclack::spinner();
			spinner.start("Calling the contract...");
			let call_dry_run_result = dry_run_call(&call_exec).await?;
			self.cli.info(format!("Result: {}", call_dry_run_result))?;
			self.cli.warning("Your call has not been executed.")?;
		} else {
			let weight_limit = if self.gas_limit.is_some() && self.proof_size.is_some() {
				Weight::from_parts(self.gas_limit.unwrap(), self.proof_size.unwrap())
			} else {
				let spinner = cliclack::spinner();
				spinner.start("Doing a dry run to estimate the gas...");
				match dry_run_gas_estimate_call(&call_exec).await {
					Ok(w) => {
						self.cli.info(format!("Gas limit: {:?}", w))?;
						w
					},
					Err(e) => {
						spinner.error(format!("{e}"));
						self.cli.outro_cancel("Call failed.")?;
						return Ok(());
					},
				}
			};
			let spinner = cliclack::spinner();
			spinner.start("Calling the contract...");

			let call_result = call_smart_contract(call_exec, weight_limit, &call_config.url)
				.await
				.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;

			self.cli.info(call_result)?;
		}
		if self.args.contract.is_none() {
			let another_call: bool = self
				.cli
				.confirm("Do you want to do another call?")
				.initial_value(false)
				.interact()?;
			if another_call {
				Box::pin(self.execute()).await?;
			} else {
				self.cli.outro("Call completed successfully!")?;
			}
		} else {
			self.cli.outro("Call completed successfully!")?;
		}
		Ok(())
	}
}

/// Guide the user to call the contract.
async fn guide_user_to_call_contract<'a, CLI: Cli>(
	command: &mut CallContract<'a, CLI>,
) -> anyhow::Result<CallContractCommand> {
	command.cli.intro("Call a contract")?;

	// Prompt for location of your contract.
	let input_path: String = command
		.cli
		.input("Where is your project located?")
		.placeholder("./")
		.default_input("./")
		.interact()?;
	let contract_path = Path::new(&input_path);
	println!("path: {:?}", contract_path);

	// Prompt for contract address.
	let contract_address: String = command
		.cli
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
			command.cli.outro_cancel("Unable to fetch contract metadata.")?;
			return Err(anyhow!(format!("{}", e.to_string())));
		},
	};
	let message = {
		let mut prompt = command.cli.select("Select the message to call:");
		for select_message in messages {
			prompt =
				prompt.item(select_message.clone(), &select_message.label, &select_message.docs);
		}
		prompt.interact()?
	};

	let mut contract_args = Vec::new();
	for arg in &message.args {
		contract_args.push(command.cli.input(arg).placeholder(arg).interact()?);
	}
	let mut value = "0".to_string();
	if message.payable {
		value = command
			.cli
			.input("Value to transfer to the call:")
			.placeholder("0")
			.default_input("0")
			.interact()?;
	}
	let mut gas_limit: Option<u64> = None;
	let mut proof_size: Option<u64> = None;
	if message.mutates {
		// Prompt for gas limit and proof_size of the call.
		let gas_limit_input: String = command
			.cli
			.input("Enter the gas limit:")
			.required(false)
			.default_input("")
			.placeholder("If left blank, an estimation will be used")
			.interact()?;
		gas_limit = gas_limit_input.parse::<u64>().ok(); // If blank or bad input, estimate it.
		let proof_size_input: String = command
			.cli
			.input("Enter the proof size limit:")
			.required(false)
			.placeholder("If left blank, an estimation will be used")
			.default_input("")
			.interact()?;
		proof_size = proof_size_input.parse::<u64>().ok(); // If blank or bad input, estimate it.
	}

	// Prompt for contract location.
	let url: String = command
		.cli
		.input("Where is your contract deployed?")
		.placeholder("ws://localhost:9944")
		.default_input("ws://localhost:9944")
		.interact()?;

	// Who is calling the contract.
	let suri: String = command
		.cli
		.input("Signer calling the contract:")
		.placeholder("//Alice")
		.default_input("//Alice")
		.interact()?;

	let mut is_call_confirmed: bool = true;
	if message.mutates {
		is_call_confirmed = command
			.cli
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use pop_contracts::{create_smart_contract, Contract};
	use std::{env, fs};
	use url::Url;

	fn generate_smart_contract_test_environment() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("testing");
		fs::create_dir(&temp_contract_dir)?;
		create_smart_contract("testing", temp_contract_dir.as_path(), &Contract::Standard)?;
		Ok(temp_dir)
	}
	// Function that mocks the build process generating the contract artifacts.
	fn mock_build_process(temp_contract_dir: PathBuf) -> Result<()> {
		// Create a target directory
		let target_contract_dir = temp_contract_dir.join("target");
		fs::create_dir(&target_contract_dir)?;
		fs::create_dir(&target_contract_dir.join("ink"))?;
		// Copy a mocked testing.contract and testing.json files inside the target directory
		let current_dir = env::current_dir().expect("Failed to get current directory");
		let contract_file = current_dir.join("../../tests/files/testing.contract");
		fs::copy(contract_file, &target_contract_dir.join("ink/testing.contract"))?;
		let metadata_file = current_dir.join("../../tests/files/testing.json");
		fs::copy(metadata_file, &target_contract_dir.join("ink/testing.json"))?;
		Ok(())
	}

	#[tokio::test]
	async fn call_contract_messages_are_ok() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;

		let mut cli = MockCli::new()
			.expect_intro(&"Call a contract")
			.expect_warning("Your call has not been executed.")
			.expect_outro("Call completed successfully!");

		// Contract deployed on Pop Network testnet, test get
		Box::new(CallContract {
			cli: &mut cli,
			args: CallContractCommand {
				path: Some(temp_dir.path().join("testing")),
				contract: Some("14BfwaXddoarT9Z6LcfXBmunutk6Ssmy1kBdWX6sy9KRskJz".to_string()),
				message: Some("get".to_string()),
				args: vec![].to_vec(),
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
				suri: "//Alice".to_string(),
				dry_run: false,
				execute: false,
			},
		})
		.execute()
		.await?;

		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually calling the contract.
	#[tokio::test]
	async fn guide_user_to_query_contract_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;

		let items = vec![
			("flip".into(), " A message that can be called on instantiated contracts.  This one flips the value of the stored `bool` from `true`  to `false` and vice versa.".into()),
			("get".into(), " Simply returns the current value of our `bool`.".into()),
		];
		// The inputs are processed in reverse order.
		let mut cli = MockCli::new()
			.expect_intro(&"Call a contract")
			.expect_input("Signer calling the contract:", "//Alice".into())
			.expect_input(
				"Where is your contract deployed?",
				"wss://rpc1.paseo.popnetwork.xyz".into(),
			)
			.expect_select::<PathBuf>(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				1, // "get" message
			)
			.expect_input(
				"Paste the on-chain contract address:",
				"14BfwaXddoarT9Z6LcfXBmunutk6Ssmy1kBdWX6sy9KRskJz".into(),
			)
			.expect_input(
				"Where is your project located?",
				temp_dir.path().join("testing").display().to_string(),
			);

		let call_config = guide_user_to_call_contract(&mut CallContract {
			cli: &mut cli,
			args: CallContractCommand {
				path: Some(temp_dir.path().join("testing")),
				contract: None,
				message: None,
				args: vec![].to_vec(),
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				url: Url::parse("ws://localhost:9944")?,
				suri: "//Alice".to_string(),
				dry_run: false,
				execute: false,
			},
		})
		.await?;
		assert_eq!(
			call_config.contract,
			Some("14BfwaXddoarT9Z6LcfXBmunutk6Ssmy1kBdWX6sy9KRskJz".to_string())
		);
		assert_eq!(call_config.message, Some("get".to_string()));
		assert_eq!(call_config.args.len(), 0);
		assert_eq!(call_config.value, "0".to_string());
		assert_eq!(call_config.gas_limit, None);
		assert_eq!(call_config.proof_size, None);
		assert_eq!(call_config.url.to_string(), "wss://rpc1.paseo.popnetwork.xyz/");
		assert_eq!(call_config.suri, "//Alice");
		assert!(!call_config.execute);
		assert!(!call_config.dry_run);

		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually calling the contract.
	#[tokio::test]
	async fn guide_user_to_call_contract_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		mock_build_process(temp_dir.path().join("testing"))?;

		let items = vec![
			("flip".into(), " A message that can be called on instantiated contracts.  This one flips the value of the stored `bool` from `true`  to `false` and vice versa.".into()),
			("get".into(), " Simply returns the current value of our `bool`.".into()),
		];
		// The inputs are processed in reverse order.
		let mut cli = MockCli::new()
			.expect_intro(&"Call a contract")
			.expect_input("Signer calling the contract:", "//Alice".into())
			.expect_input(
				"Where is your contract deployed?",
				"wss://rpc1.paseo.popnetwork.xyz".into(),
			)
			.expect_select::<PathBuf>(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				0, // "flip" message
			)
			.expect_input("Enter the proof size limit:", "".into())
			.expect_input("Enter the gas limit:", "".into())
			.expect_input(
				"Paste the on-chain contract address:",
				"14BfwaXddoarT9Z6LcfXBmunutk6Ssmy1kBdWX6sy9KRskJz".into(),
			)
			.expect_input(
				"Where is your project located?",
				temp_dir.path().join("testing").display().to_string(),
			);

		let call_config = guide_user_to_call_contract(&mut CallContract {
			cli: &mut cli,
			args: CallContractCommand {
				path: Some(temp_dir.path().join("testing")),
				contract: None,
				message: None,
				args: vec![].to_vec(),
				value: "0".to_string(),
				gas_limit: None,
				proof_size: None,
				url: Url::parse("ws://localhost:9944")?,
				suri: "//Alice".to_string(),
				dry_run: false,
				execute: false,
			},
		})
		.await?;
		assert_eq!(
			call_config.contract,
			Some("14BfwaXddoarT9Z6LcfXBmunutk6Ssmy1kBdWX6sy9KRskJz".to_string())
		);
		assert_eq!(call_config.message, Some("flip".to_string()));
		assert_eq!(call_config.args.len(), 0);
		assert_eq!(call_config.value, "0".to_string());
		assert_eq!(call_config.gas_limit, None);
		assert_eq!(call_config.proof_size, None);
		assert_eq!(call_config.url.to_string(), "wss://rpc1.paseo.popnetwork.xyz/");
		assert_eq!(call_config.suri, "//Alice");
		assert!(call_config.execute);
		assert!(!call_config.dry_run);

		cli.verify()
	}
}
