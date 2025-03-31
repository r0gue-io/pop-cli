use crate::{
	cli,
	common::{
		prompt::display_message,
		try_runtime::{
			check_try_runtime_and_prompt, collect_state_arguments, update_live_state,
			ArgumentConstructor,
		},
	},
};
use clap::Args;
use cliclack::spinner;
use frame_try_runtime::TryStateSelect;
use pop_parachains::{
	parse_try_state_string, run_try_runtime,
	state::{LiveState, State, StateCommand},
	TryRuntimeCliCommand,
};

const CUSTOM_ARGS: [&str; 2] = ["--skip-confirm", "-y"];

#[derive(Args, Default)]
pub(crate) struct TestExecuteBlockCommand {
	/// Which try-state targets to execute when running this command.
	///
	/// Expected values:
	/// - `all`
	/// - `none`
	/// - A comma separated list of pallets, as per pallet names in `construct_runtime!()` (e.g.
	///   `Staking, System`).
	/// - `rr-[x]` where `[x]` is a number. Then, the given number of pallets are checked in a
	///   round-robin fashion.
	#[arg(long, default_value = "all")]
	try_state: TryStateSelect,

	/// The state to use.
	#[command(flatten)]
	state: LiveState,

	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl TestExecuteBlockCommand {
	pub(crate) async fn execute(mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Testing a block execution.")?;
		// Prompt the user to select the source of runtime state.
		if let Err(e) = update_live_state(cli, &mut self.state, &mut None) {
			return display_message(&e.to_string(), false, cli);
		};
		// Test block execution with `try-runtime-cli` binary.
		let result = self.run(cli).await;

		// Display the `execute-block` command.
		cli.info(self.display()?)?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Tested block execution successfully!", true, cli)
	}

	async fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		cli.warning("NOTE: this may take some time...")?;

		let spinner = spinner();
		spinner.start("Executing block...");
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let (before_subcommand, after_subcommand) = self.collect_arguments(user_provided_args)?;
		run_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::ExecuteBlock,
			vec![],
			[before_subcommand, vec![StateCommand::Live.to_string()], after_subcommand].concat(),
			&CUSTOM_ARGS,
		)?;
		spinner.stop("");
		Ok(())
	}

	fn display(&self) -> anyhow::Result<String> {
		let mut cmd_args = vec!["pop test execute-block".to_string()];
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let (before_command_args, after_subcommand_args) =
			self.collect_arguments(user_provided_args)?;
		cmd_args.extend(before_command_args);
		cmd_args.extend(after_subcommand_args);
		Ok(cmd_args.join(" "))
	}

	// Handle arguments before the subcommand.
	fn collect_arguments(
		&self,
		user_provided_args: Vec<String>,
	) -> anyhow::Result<(Vec<String>, Vec<String>)> {
		let mut before_subcommand = vec![];
		let mut after_subcommand = vec![];
		for arg in user_provided_args.into_iter() {
			if [vec!["--try-state"], CUSTOM_ARGS.to_vec()]
				.concat()
				.iter()
				.any(|a| arg.starts_with(a))
			{
				before_subcommand.push(arg);
			} else {
				after_subcommand.push(arg);
			}
		}

		let mut before_command_args = vec![];
		let mut c = ArgumentConstructor::new(&mut before_command_args, &before_subcommand);
		c.add(&[], true, "--try-state", Some(parse_try_state_string(self.try_state.clone())?));
		// These are custom arguments not used in `try-runtime-cli`.
		c.add(&["--skip-confirm"], self.skip_confirm, "-y", Some(String::default()));
		c.finalize(&["--at="]);

		let mut after_subcommand_args = vec![];
		collect_state_arguments(
			&Some(State::Live(self.state.clone())),
			&after_subcommand,
			&mut after_subcommand_args,
		)?;
		Ok((before_command_args, after_subcommand_args))
	}
}

#[cfg(test)]
mod tests {
	use super::TestExecuteBlockCommand;
	use crate::{
		cli::MockCli,
		common::try_runtime::{
			source_try_runtime_binary, DEFAULT_BLOCK_HASH, DEFAULT_LIVE_NODE_URL,
		},
	};
	use frame_try_runtime::TryStateSelect;
	use pop_parachains::parse_try_state_string;

	#[tokio::test]
	async fn test_execute_block_works() -> anyhow::Result<()> {
		source_try_runtime_binary(&mut MockCli::new(), &crate::cache()?, true).await?;
		let mut cli = MockCli::new()
			.expect_intro("Testing a block execution.")
			.expect_input("Enter the live chain of your node:", DEFAULT_LIVE_NODE_URL.to_string())
			.expect_input("Enter the block hash (optional):", String::default())
			.expect_warning("NOTE: This may take some time...")
			.expect_info(format!(
				"pop test execute-block --try-state={} --uri={}",
				parse_try_state_string(TryStateSelect::None)?,
				DEFAULT_LIVE_NODE_URL,
			))
			.expect_outr_cancel("thread 'main' panicked at cli/main.rs:326:10:\n\
			called `Result::unwrap()` on an `Err` value: Input(\"Given runtime is not compiled with the try-runtime feature.\")\n\
			note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace");
		let error = TestExecuteBlockCommand::default().execute(&mut cli).await.unwrap_err();
		// The error is expected because `pop-node` production runtime on Paseo is not compiled with the `try-runtime` feature.
		assert!(error.to_string().contains("Given runtime is not compiled with the try-runtime feature."));
		cli.verify()
	}

	#[tokio::test]
	async fn test_execute_block_invalid_header() -> anyhow::Result<()> {
		source_try_runtime_binary(&mut MockCli::new(), &crate::cache()?, true).await?;
		let mut command = TestExecuteBlockCommand::default();
		command.state.uri = Some(DEFAULT_LIVE_NODE_URL.to_string());
		command.state.at = Some(DEFAULT_BLOCK_HASH.to_string());
		let error = command.run(&mut MockCli::new()).await.unwrap_err();
		assert!(error.to_string().contains("header_not_found"));
		Ok(())
	}

	#[tokio::test]
	async fn test_execute_block_invalid_uri() -> anyhow::Result<()> {
		source_try_runtime_binary(&mut MockCli::new(), &crate::cache()?, true).await?;
		let mut command = TestExecuteBlockCommand::default();
		command.state.uri = Some("ws://localhost:9945".to_string());
		let error = command.run(&mut MockCli::new()).await.unwrap_err();
		assert!(error.to_string().contains("Connection refused"));
		Ok(())
	}

	#[test]
	fn display_works() -> anyhow::Result<()> {
		let mut command = TestExecuteBlockCommand::default();
		command.try_state = TryStateSelect::RoundRobin(10);
		command.state.uri = Some(DEFAULT_LIVE_NODE_URL.to_string());
		command.skip_confirm = true;
		assert_eq!(
			command.display()?,
			format!(
				"pop test execute-block --try-state=rr-10 -y --uri={}",
				DEFAULT_LIVE_NODE_URL.to_string()
			)
		);
		Ok(())
	}

	#[test]
	fn collect_arguments_works() -> anyhow::Result<()> {
		let test_cases: Vec<(bool, &str, Box<dyn Fn(&mut TestExecuteBlockCommand)>, &str)> = vec![
			(
				true,
				"--try-state=all",
				Box::new(|cmd| {
					cmd.try_state = TryStateSelect::RoundRobin(10);
				}),
				"--try-state=rr-10",
			),
			(
				true,
				"-y",
				Box::new(|cmd| {
					cmd.skip_confirm = true;
				}),
				"-y",
			),
			(
				true,
				"--skip-confirm",
				Box::new(|cmd| {
					cmd.skip_confirm = true;
				}),
				"-y",
			),
			(
				false,
				"--uri=ws://127.0.0.1:9944",
				Box::new(|cmd| cmd.state.uri = Some("ws://127.0.0.1:9945".to_string())),
				"--uri=ws://127.0.0.1:9945",
			),
			(
				false,
				"--at=1000000",
				Box::new(|cmd| cmd.state.at = Some("1200000".to_string())),
				"--at=1200000",
			),
		];
		for (test_before, provided_arg, update_fn, expected_arg) in test_cases {
			let mut command = TestExecuteBlockCommand::default();
			// Keep the user-provided argument unchanged.
			let (before, after) = command.collect_arguments(vec![provided_arg.to_string()])?;
			assert_eq!(
				if test_before { before.clone() } else { after.clone() }
					.iter()
					.filter(|a| a.contains(&provided_arg.to_string()))
					.count(),
				1
			);

			// If there exists an argument with the same name as the provided argument, skip it.
			command.collect_arguments(vec![])?;
			assert_eq!(
				if test_before { before } else { after }
					.iter()
					.filter(|a| a.contains(&provided_arg.to_string()))
					.count(),
				1
			);

			// If the user does not provide an argument, modify with the argument updated during
			// runtime.
			update_fn(&mut command);
			let (before, after) = command.collect_arguments(vec![])?;
			assert_eq!(
				if test_before { before } else { after }
					.iter()
					.filter(|a| a.contains(&expected_arg.to_string()))
					.count(),
				1
			);
		}
		Ok(())
	}
}
