// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::{anyhow, Result};
use clap::Args;
use pop_common::parse_account;
use pop_contracts::{
	call_smart_contract, dry_run_call, dry_run_gas_estimate_call, get_messages, set_up_call,
	CallOpts,
};
use sp_weights::Weight;
use std::path::PathBuf;

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
	/// The value to be transferred as part of the call.
	#[clap(name = "value", long, default_value = "0")]
	value: String,
	/// Maximum amount of gas to be used for this command.
	/// If not specified it will perform a dry-run to estimate the gas consumed for the
	/// call.
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
impl CallContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<()> {
		let call_config: CallContractCommand = match self.set_up_call_config(&mut cli::Cli).await {
			Ok(call_config) => call_config,
			Err(e) => {
				display_message(&e.to_string(), false, &mut cli::Cli)?;
				return Ok(());
			},
		};
		match execute_call(call_config, self.contract.is_none(), &mut cli::Cli).await {
			Ok(_) => Ok(()),
			Err(e) => {
				display_message(&e.to_string(), false, &mut cli::Cli)?;
				Ok(())
			},
		}
	}

	fn display(&self) -> String {
		let mut full_message = "pop call contract".to_string();
		if let Some(path) = &self.path {
			full_message.push_str(&format!(" --path {}", path.display()));
		}
		if let Some(contract) = &self.contract {
			full_message.push_str(&format!(" --contract {}", contract));
		}
		if let Some(message) = &self.message {
			full_message.push_str(&format!(" --message {}", message));
		}
		if !self.args.is_empty() {
			full_message.push_str(&format!(" --args {}", self.args.join(" ")));
		}
		if self.value != "0" {
			full_message.push_str(&format!(" --value {}", self.value));
		}
		if let Some(gas_limit) = self.gas_limit {
			full_message.push_str(&format!(" --gas {}", gas_limit));
		}
		if let Some(proof_size) = self.proof_size {
			full_message.push_str(&format!(" --proof_size {}", proof_size));
		}
		full_message.push_str(&format!(" --url {} --suri {}", self.url, self.suri));
		if self.execute {
			full_message.push_str(" --execute");
		}
		if self.dry_run {
			full_message.push_str(" --dry_run");
		}
		full_message
	}

	/// Set up the config call.
	async fn set_up_call_config(
		&self,
		cli: &mut impl cli::traits::Cli,
	) -> anyhow::Result<CallContractCommand> {
		cli.intro("Call a contract")?;
		let call_config = if self.contract.is_none() {
			match guide_user_to_call_contract(None, None, None, cli).await {
				Ok(config) => config,
				Err(e) => {
					return Err(anyhow!(format!("{}", e.to_string())));
				},
			}
		} else {
			self.clone()
		};
		Ok(call_config)
	}
}

/// Guide the user to call the contract.
async fn guide_user_to_call_contract(
	contract_path: Option<PathBuf>,
	url: Option<url::Url>,
	contract_address: Option<String>,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<CallContractCommand> {
	let contract_path: PathBuf = match contract_path {
		Some(path) => path,
		None => {
			// Prompt for path.
			let input_path: String = cli
				.input("Where is your project located?")
				.placeholder("./")
				.default_input("./")
				.interact()?;
			PathBuf::from(input_path)
		},
	};
	// Parse the contract metadata provided. If there error, do not prompt for more.
	let messages = match get_messages(&contract_path) {
		Ok(messages) => messages,
		Err(e) => {
			return Err(anyhow!(format!(
				"Unable to fetch contract metadata: {}",
				e.to_string().replace("Anyhow error: ", "")
			)));
		},
	};
	let url: url::Url = match url {
		Some(url) => url,
		None => {
			// Prompt for url.
			let url: String = cli
				.input("Where is your contract deployed?")
				.placeholder("ws://localhost:9944")
				.default_input("ws://localhost:9944")
				.interact()?;
			url::Url::parse(&url)?
		},
	};
	let contract_address: String = match contract_address {
		Some(contract_address) => contract_address,
		None => {
			// Prompt for contract address.
			let contract_address: String = cli
				.input("Paste the on-chain contract address:")
				.placeholder("e.g. 5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
				.validate(|input: &String| match parse_account(input) {
					Ok(_) => Ok(()),
					Err(_) => Err("Invalid address."),
				})
				.default_input("5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
				.interact()?;
			contract_address
		},
	};

	let message = {
		let mut prompt = cli.select("Select the message to call:");
		for select_message in messages {
			prompt = prompt.item(
				select_message.clone(),
				format!("{}\n", &select_message.label),
				&select_message.docs,
			);
		}
		prompt.interact()?
	};

	let mut contract_args = Vec::new();
	for arg in &message.args {
		contract_args.push(
			cli.input(format!("Enter the value for the parameter: {}", arg.label))
				.placeholder(&format!("Type required: {}", &arg.type_name))
				.interact()?,
		);
	}
	let mut value = "0".to_string();
	if message.payable {
		value = cli
			.input("Value to transfer to the call:")
			.placeholder("0")
			.default_input("0")
			.validate(|input: &String| match input.parse::<u64>() {
				Ok(_) => Ok(()),
				Err(_) => Err("Invalid value."),
			})
			.interact()?;
	}
	let mut gas_limit: Option<u64> = None;
	let mut proof_size: Option<u64> = None;
	if message.mutates {
		// Prompt for gas limit and proof_size of the call.
		let gas_limit_input: String = cli
			.input("Enter the gas limit:")
			.required(false)
			.default_input("")
			.placeholder("If left blank, an estimation will be used")
			.interact()?;
		gas_limit = gas_limit_input.parse::<u64>().ok(); // If blank or bad input, estimate it.
		let proof_size_input: String = cli
			.input("Enter the proof size limit:")
			.required(false)
			.placeholder("If left blank, an estimation will be used")
			.default_input("")
			.interact()?;
		proof_size = proof_size_input.parse::<u64>().ok(); // If blank or bad input, estimate it.
	}

	// Who is calling the contract.
	let suri: String = cli
		.input("Signer calling the contract:")
		.placeholder("//Alice")
		.default_input("//Alice")
		.interact()?;

	let mut is_call_confirmed: bool = true;
	if message.mutates {
		is_call_confirmed = cli
			.confirm("Do you want to execute the call? (Selecting 'No' will perform a dry run)")
			.initial_value(true)
			.interact()?;
	}
	let call_command = CallContractCommand {
		path: Some(contract_path),
		contract: Some(contract_address),
		message: Some(message.label.clone()),
		args: contract_args,
		value,
		gas_limit,
		proof_size,
		url,
		suri,
		execute: if is_call_confirmed { message.mutates } else { false },
		dry_run: !is_call_confirmed,
	};
	cli.info(call_command.display())?;
	Ok(call_command)
}

/// Executes the call.
async fn execute_call(
	call_config: CallContractCommand,
	prompt_to_repeat_call: bool,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	let contract = call_config
		.contract
		.clone()
		.expect("contract can not be none as fallback above is interactive input; qed");
	let message = match call_config.message {
		Some(m) => m,
		None => {
			return Err(anyhow!("Please specify the message to call."));
		},
	};

	let call_exec = match set_up_call(CallOpts {
		path: call_config.path.clone(),
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
	.await
	{
		Ok(call_exec) => call_exec,
		Err(e) => {
			return Err(anyhow!(format!("{}", e.root_cause().to_string())));
		},
	};

	if call_config.dry_run {
		let spinner = cliclack::spinner();
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

	if !call_config.execute {
		let spinner = cliclack::spinner();
		spinner.start("Calling the contract...");
		let call_dry_run_result = dry_run_call(&call_exec).await?;
		cli.info(format!("Result: {}", call_dry_run_result))?;
		cli.warning("Your call has not been executed.")?;
	} else {
		let weight_limit = if call_config.gas_limit.is_some() && call_config.proof_size.is_some() {
			Weight::from_parts(call_config.gas_limit.unwrap(), call_config.proof_size.unwrap())
		} else {
			let spinner = cliclack::spinner();
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
		let spinner = cliclack::spinner();
		spinner.start("Calling the contract...");

		let call_result = call_smart_contract(call_exec, weight_limit, &call_config.url)
			.await
			.map_err(|err| anyhow!("{} {}", "ERROR:", format!("{err:?}")))?;

		cli.info(call_result)?;
	}
	if prompt_to_repeat_call {
		let another_call: bool = cli
			.confirm("Do you want to do another call using the existing smart contract?")
			.initial_value(false)
			.interact()?;
		if another_call {
			// Remove only the prompt asking for another call.
			console::Term::stderr().clear_last_lines(2)?;
			let new_call_config = guide_user_to_call_contract(
				call_config.path,
				Some(call_config.url),
				call_config.contract,
				cli,
			)
			.await?;
			Box::pin(execute_call(new_call_config, prompt_to_repeat_call, cli)).await?;
		} else {
			display_message("Call completed successfully!", true, cli)?;
		}
	} else {
		display_message("Call completed successfully!", true, cli)?;
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;
	use pop_contracts::{generate_smart_contract_test_environment, mock_build_process};
	use std::env;
	use url::Url;

	#[tokio::test]
	async fn execute_query_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
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
			contract: Some("15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm".to_string()),
			message: Some("get".to_string()),
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: "//Alice".to_string(),
			dry_run: false,
			execute: false,
		}
		.execute()
		.await?;
		Ok(())
	}

	#[tokio::test]
	async fn call_contract_dry_run_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
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

		let call_config = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			contract: Some("15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm".to_string()),
			message: Some("flip".to_string()),
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: Some(100),
			proof_size: Some(10),
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: "//Alice".to_string(),
			dry_run: true,
			execute: false,
		}
		.set_up_call_config(&mut cli)
		.await?;
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract 15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm --message flip --gas 100 --proof_size 10 --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Alice --dry_run",
			temp_dir.path().join("testing").display().to_string(),
		));
		// Contract deployed on Pop Network testnet, test dry-run
		execute_call(call_config, false, &mut cli).await?;

		cli.verify()
	}

	#[tokio::test]
	async fn call_contract_query_duplicate_call_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
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
				"Do you want to do another call using the existing smart contract?",
				false,
			)
			.expect_confirm(
				"Do you want to do another call using the existing smart contract?",
				true,
			)
			.expect_select::<PathBuf>(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				1, // "get" message
			)
			.expect_input("Signer calling the contract:", "//Alice".into())
			.expect_info(format!(
			    "pop call contract --path {} --contract 15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm --message get --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Alice",
			    temp_dir.path().join("testing").display().to_string(),
			))
			.expect_warning("Your call has not been executed.")
			.expect_outro("Call completed successfully!");

		// Contract deployed on Pop Network testnet, test get
		let config_call = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			contract: Some("15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm".to_string()),
			message: Some("get".to_string()),
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: "//Alice".to_string(),
			dry_run: false,
			execute: false,
		}
		.set_up_call_config(&mut cli)
		.await?;
		// Test the query. With true, it will prompt for another call.
		execute_call(config_call, true, &mut cli).await?;

		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually
	// calling the contract.
	#[tokio::test]
	async fn guide_user_to_query_contract_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
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
			.expect_input("Signer calling the contract:", "//Alice".into())
			.expect_select::<PathBuf>(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				1, // "get" message
			)
			.expect_input(
				"Paste the on-chain contract address:",
				"15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm".into(),
			)
			.expect_input(
				"Where is your contract deployed?",
				"wss://rpc1.paseo.popnetwork.xyz".into(),
			)
			.expect_input(
				"Where is your project located?",
				temp_dir.path().join("testing").display().to_string(),
			).expect_info(format!(
	            "pop call contract --path {} --contract 15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm --message get --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Alice",
	            temp_dir.path().join("testing").display().to_string(),
	        ));

		let call_config = guide_user_to_call_contract(None, None, None, &mut cli).await?;
		assert_eq!(
			call_config.contract,
			Some("15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm".to_string())
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
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract 15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm --message get --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Alice",
			temp_dir.path().join("testing").display().to_string(),
		));

		cli.verify()
	}

	// This test only covers the interactive portion of the call contract command, without actually
	// calling the contract.
	#[tokio::test]
	async fn guide_user_to_call_contract_works() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
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
			.expect_confirm("Do you want to execute the call? (Selecting 'No' will perform a dry run)", true)
			.expect_input("Signer calling the contract:", "//Alice".into())
			.expect_input("Enter the proof size limit:", "".into()) // Only if call
			.expect_input("Enter the gas limit:", "".into()) // Only if call
			.expect_input("Value to transfer to the call:", "50".into()) // Only if payable
			.expect_input("Enter the value for the parameter: new_value", "true".into()) // Args for specific_flip
			.expect_select::<PathBuf>(
				"Select the message to call:",
				Some(false),
				true,
				Some(items),
				2, // "specific_flip" message
			)
			.expect_input(
				"Paste the on-chain contract address:",
				"15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm".into(),
			)
			.expect_input(
				"Where is your contract deployed?",
				"wss://rpc1.paseo.popnetwork.xyz".into(),
			)
			.expect_input(
				"Where is your project located?",
				temp_dir.path().join("testing").display().to_string(),
			).expect_info(format!(
				"pop call contract --path {} --contract 15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm --message specific_flip --args true --value 50 --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Alice --execute",
				temp_dir.path().join("testing").display().to_string(),
			));

		let call_config = guide_user_to_call_contract(None, None, None, &mut cli).await?;
		assert_eq!(
			call_config.contract,
			Some("15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm".to_string())
		);
		assert_eq!(call_config.message, Some("specific_flip".to_string()));
		assert_eq!(call_config.args.len(), 1);
		assert_eq!(call_config.args[0], "true".to_string());
		assert_eq!(call_config.value, "50".to_string());
		assert_eq!(call_config.gas_limit, None);
		assert_eq!(call_config.proof_size, None);
		assert_eq!(call_config.url.to_string(), "wss://rpc1.paseo.popnetwork.xyz/");
		assert_eq!(call_config.suri, "//Alice");
		assert!(call_config.execute);
		assert!(!call_config.dry_run);
		assert_eq!(call_config.display(), format!(
			"pop call contract --path {} --contract 15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm --message specific_flip --args true --value 50 --url wss://rpc1.paseo.popnetwork.xyz/ --suri //Alice --execute",
			temp_dir.path().join("testing").display().to_string(),
		));

		cli.verify()
	}

	#[tokio::test]
	async fn guide_user_to_call_contract_fails_not_build() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		let mut cli = MockCli::new();
		assert!(
			matches!(guide_user_to_call_contract(Some(temp_dir.path().join("testing")), None, None, &mut cli).await, anyhow::Result::Err(message) if message.to_string().contains("Unable to fetch contract metadata: Failed to find any contract artifacts in target directory."))
		);
		cli.verify()
	}

	#[tokio::test]
	async fn call_contract_fails_no_message() -> Result<()> {
		let temp_dir = generate_smart_contract_test_environment()?;
		let mut current_dir = env::current_dir().expect("Failed to get current directory");
		current_dir.pop();
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("pop-contracts/tests/files/testing.contract"),
			current_dir.join("pop-contracts/tests/files/testing.json"),
		)?;

		let mut cli = MockCli::new().expect_intro(&"Call a contract");

		let call_config = CallContractCommand {
			path: Some(temp_dir.path().join("testing")),
			contract: Some("15XausWjFLBBFLDXUSBRfSfZk25warm4wZRV4ZxhZbfvjrJm".to_string()),
			message: None,
			args: vec![].to_vec(),
			value: "0".to_string(),
			gas_limit: None,
			proof_size: None,
			url: Url::parse("wss://rpc1.paseo.popnetwork.xyz")?,
			suri: "//Alice".to_string(),
			dry_run: false,
			execute: false,
		}
		.set_up_call_config(&mut cli)
		.await?;
		assert!(matches!(
			execute_call(call_config, false, &mut cli).await,
			anyhow::Result::Err(message) if message.to_string() == "Please specify the message to call."
		));

		cli.verify()
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
}
