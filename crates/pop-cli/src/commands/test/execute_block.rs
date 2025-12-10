// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli,
	common::{
		prompt::display_message,
		try_runtime::{
			ArgumentConstructor, BuildRuntimeParams, check_try_runtime_and_prompt, collect_args,
			collect_shared_arguments, collect_state_arguments, guide_user_to_select_try_state,
			update_live_state, update_runtime_source,
		},
	},
};
use clap::Args;
use cliclack::spinner;
use pop_chains::{
	SharedParams, TryRuntimeCliCommand, parse_try_state_string, run_try_runtime,
	state::{LiveState, State, StateCommand},
	try_runtime::TryStateSelect,
};
use serde::Serialize;

// Custom arguments which are not in `try-runtime execute-block`.
const CUSTOM_ARGS: [&str; 5] = ["--profile", "--no-build", "-n", "--skip-confirm", "-y"];

#[derive(Args, Default, Serialize)]
pub(crate) struct TestExecuteBlockCommand {
	/// The state to use.
	#[command(flatten)]
	state: LiveState,

	/// Which try-state targets to execute when running this command.
	///
	/// Expected values:
	/// - `all`
	/// - `none`
	/// - A comma separated list of pallets, as per pallet names in `construct_runtime!()` (e.g.
	///   `Staking, System`).
	/// - `rr-[x]` where `[x]` is a number. Then, the given number of pallets are checked in a
	///   round-robin fashion.
	#[serde(skip_serializing)]
	#[arg(long)]
	try_state: Option<TryStateSelect>,

	/// Shared params of the try-runtime commands.
	#[clap(flatten)]
	shared_params: SharedParams,

	/// Build parameters for the runtime binary.
	#[command(flatten)]
	build_params: BuildRuntimeParams,
}

impl TestExecuteBlockCommand {
	pub(crate) async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		self.execute_block(cli, std::env::args().skip(3).collect()).await
	}

	async fn execute_block(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		user_provided_args: Vec<String>,
	) -> anyhow::Result<()> {
		cli.intro("Testing block execution")?;
		if let Err(e) = update_runtime_source(
			cli,
			"Do you want to specify which runtime to execute block on?",
			&user_provided_args,
			&mut self.shared_params.runtime,
			&mut self.build_params.profile,
			self.build_params.no_build,
		)
		.await
		{
			return display_message(&e.to_string(), false, cli);
		}

		// Prompt the update the live state.
		if let Err(e) = update_live_state(cli, &mut self.state, &mut None) {
			return display_message(&e.to_string(), false, cli);
		};

		// Prompt the user to select the try state if no `--try-state` argument is provided.
		if self.try_state.is_none() {
			let uri = self
				.state
				.uri
				.as_ref()
				.ok_or_else(|| anyhow::anyhow!("No live node URI is provided"))?;
			self.try_state = Some(guide_user_to_select_try_state(cli, Some(uri.clone())).await?);
		}

		// Test block execution with `try-runtime-cli` binary.
		let result = self.run(cli, user_provided_args.clone()).await;

		// Display the `execute-block` command.
		cli.info(self.display(user_provided_args)?)?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Block executed successfully!", true, cli)
	}

	async fn run(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		user_provided_args: Vec<String>,
	) -> anyhow::Result<()> {
		let binary_path =
			check_try_runtime_and_prompt(cli, &spinner(), self.build_params.skip_confirm).await?;
		cli.warning("NOTE: this may take some time...")?;

		let spinner = spinner();
		spinner.start("Executing block...");
		let (shared_params, before_subcommand, after_subcommand) =
			self.collect_arguments(user_provided_args)?;
		run_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::ExecuteBlock,
			shared_params,
			[before_subcommand, vec![StateCommand::Live.to_string()], after_subcommand].concat(),
			&CUSTOM_ARGS,
		)?;
		spinner.clear();
		Ok(())
	}

	fn display(&self, user_provided_args: Vec<String>) -> anyhow::Result<String> {
		let mut cmd_args = vec!["pop test execute-block".to_string()];
		let (shared_params, before_command_args, after_subcommand_args) =
			self.collect_arguments(user_provided_args)?;
		cmd_args.extend(shared_params);
		cmd_args.extend(before_command_args);
		cmd_args.extend(after_subcommand_args);
		Ok(cmd_args.join(" "))
	}

	// Handle arguments before the subcommand.
	fn collect_arguments(
		&self,
		user_provided_args: Vec<String>,
	) -> anyhow::Result<(Vec<String>, Vec<String>, Vec<String>)> {
		let (mut shared_arguments, mut before_subcommand, mut after_subcommand) =
			(vec![], vec![], vec![]);
		for arg in collect_args(user_provided_args.into_iter()) {
			if SharedParams::has_argument(&arg) {
				shared_arguments.push(arg);
			} else if is_before_subcommand(&arg) {
				before_subcommand.push(arg);
			} else {
				after_subcommand.push(arg);
			}
		}

		// Collect shared arguments.
		let mut shared_args = vec![];
		collect_shared_arguments(&self.shared_params, &shared_arguments, &mut shared_args);

		// Collect before subcommand arguments.
		let mut before_subcommand_args = vec![];
		let mut c = ArgumentConstructor::new(&mut before_subcommand_args, &before_subcommand);
		if let Some(ref try_state) = self.try_state {
			c.add(&[], true, "--try-state", Some(parse_try_state_string(try_state)?));
		}
		self.build_params.add_arguments(&mut c);
		c.finalize(&["--at="]);

		// Collect after subcommand arguments.
		let mut after_subcommand_args = vec![];
		collect_state_arguments(
			&Some(State::Live(self.state.clone())),
			&after_subcommand,
			&mut after_subcommand_args,
		)?;
		Ok((shared_args, before_subcommand_args, after_subcommand_args))
	}
}

fn is_before_subcommand(arg: &str) -> bool {
	[vec!["--try-state"], CUSTOM_ARGS.to_vec()]
		.concat()
		.iter()
		.any(|a| arg.starts_with(a))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		cli::MockCli,
		common::{
			runtime::{Feature, get_mock_runtime},
			try_runtime::{get_try_state_items, source_try_runtime_binary},
			urls,
		},
	};
	use console::style;
	use pop_common::Profile;

	#[tokio::test]
	async fn execute_block_works() -> anyhow::Result<()> {
		source_try_runtime_binary(&mut MockCli::new(), &spinner(), &crate::cache()?, true).await?;

		let mut cli = MockCli::new()
			.expect_intro("Testing block execution")
			.expect_confirm(
				format!(
					"Do you want to specify which runtime to execute block on?\n{}",
					style("If not provided, use the code of the remote node, or a snapshot.").dim()
				),
				true,
			)
			.expect_select(
				"Choose the build profile of the binary that should be used: ".to_string(),
				Some(true),
				true,
				Some(Profile::get_variants()),
				0,
				None,
			)
			.expect_warning("NOTE: Make sure your runtime is built with `try-runtime` feature.")
			.expect_warning(format!(
				"No runtime folder found at {}. Please input the runtime path manually.",
				std::env::current_dir()?.display()
			))
			.expect_input(
				"Please, specify the path to the runtime project or the runtime binary.",
				get_mock_runtime(Some(Feature::TryRuntime)).to_str().unwrap().to_string(),
			)
			.expect_info(format!(
				"Using runtime at {}",
				get_mock_runtime(Some(Feature::TryRuntime)).display()
			))
			.expect_input("Enter the live chain of your node:", urls::LOCAL.to_string())
			.expect_input("Enter the block hash (optional):", String::default())
			.expect_select(
				"Select state tests to execute:",
				Some(true),
				true,
				Some(get_try_state_items()),
				1,
				None,
			);
		let mut command = TestExecuteBlockCommand::default();
		command.build_params.no_build = true;
		command.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn execute_block_invalid_uri() -> anyhow::Result<()> {
		source_try_runtime_binary(&mut MockCli::new(), &spinner(), &crate::cache()?, true).await?;
		let mut cmd = TestExecuteBlockCommand::default();
		cmd.state.uri = Some("ws://127.0.0.1:9999".to_string());
		let error = cmd.run(&mut MockCli::new(), vec![]).await.unwrap_err();
		assert!(error.to_string().contains("Connection refused"), "Unexpected error: {}", error);
		Ok(())
	}

	#[test]
	fn display_works() -> anyhow::Result<()> {
		let mut cmd = TestExecuteBlockCommand {
			try_state: Some(TryStateSelect::RoundRobin(10)),
			..Default::default()
		};
		cmd.state.uri = Some(urls::LOCAL.to_string());
		cmd.build_params.skip_confirm = true;
		assert_eq!(
			cmd.display(vec![])?,
			format!(
				"pop test execute-block --runtime=existing --try-state=rr-10 -y --uri={}",
				urls::LOCAL
			)
		);
		assert_eq!(
			cmd.display(vec![
				"--runtime".to_string(),
				"existing".to_string(),
				"--try-state".to_string(),
				"rr-10".to_string(),
				"-n".to_string()
			])?,
			format!(
				"pop test execute-block --runtime=existing -y --try-state=rr-10 -n --uri={}",
				urls::LOCAL
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
					cmd.try_state = Some(TryStateSelect::RoundRobin(10));
				}),
				"--try-state=rr-10",
			),
			(
				true,
				"-y",
				Box::new(|cmd| {
					cmd.build_params.skip_confirm = true;
				}),
				"-y",
			),
			(
				true,
				"--skip-confirm",
				Box::new(|cmd| {
					cmd.build_params.skip_confirm = true;
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
			let (_, before, after) = command.collect_arguments(vec![provided_arg.to_string()])?;
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
			let (_, before, after) = command.collect_arguments(vec![])?;
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
