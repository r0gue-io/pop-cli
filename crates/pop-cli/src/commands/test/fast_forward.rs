// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self,
		traits::{Confirm, Input},
	},
	common::{
		prompt::display_message,
		try_runtime::{
			ArgumentConstructor, BuildRuntimeParams, DEFAULT_BLOCK_TIME,
			check_try_runtime_and_prompt, collect_args, collect_shared_arguments,
			collect_state_arguments, guide_user_to_select_try_state, partition_arguments,
			update_runtime_source, update_state_source,
		},
	},
};
use clap::Args;
use cliclack::spinner;
use console::style;
use pop_chains::{
	SharedParams, TryRuntimeCliCommand, parse_try_state_string, run_try_runtime,
	state::{LiveState, State, StateCommand},
	try_runtime::TryStateSelect,
};
use serde::Serialize;

// Custom arguments which are not in `try-runtime fast-forward`.
const CUSTOM_ARGS: [&str; 5] = ["--profile", "--no-build", "-n", "--skip-confirm", "-y"];
const DEFAULT_N_BLOCKS: u64 = 10;

#[derive(Args, Serialize)]
pub(crate) struct TestFastForwardCommand {
	/// The state to use.
	#[command(subcommand)]
	state: Option<State>,

	/// How many empty blocks should be processed.
	#[arg(long)]
	n_blocks: Option<u64>,

	/// The chain blocktime in milliseconds.
	#[arg(long, default_value = &DEFAULT_BLOCK_TIME.to_string())]
	blocktime: u64,

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

	/// Whether to run pending migrations before fast-forwarding.
	#[arg(long)]
	run_migrations: bool,

	/// Shared params of the try-runtime commands.
	#[clap(flatten)]
	shared_params: SharedParams,

	/// Build parameters for the runtime binary.
	#[command(flatten)]
	build_params: BuildRuntimeParams,
}

#[cfg(test)]
impl Default for TestFastForwardCommand {
	fn default() -> Self {
		Self {
			state: None,
			n_blocks: None,
			blocktime: DEFAULT_BLOCK_TIME,
			try_state: None,
			run_migrations: false,
			shared_params: SharedParams::default(),
			build_params: BuildRuntimeParams::default(),
		}
	}
}

impl TestFastForwardCommand {
	pub(crate) async fn execute(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let user_provided_args = collect_args(std::env::args().skip(3));
		self.fast_forward(cli, &user_provided_args).await
	}

	async fn fast_forward(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		user_provided_args: &[String],
	) -> anyhow::Result<()> {
		cli.intro("Performing try-state checks on simulated block execution")?;
		if let Err(e) = update_runtime_source(
			cli,
			"Do you want to specify which runtime to perform try-state checks on?",
			user_provided_args,
			&mut self.shared_params.runtime,
			&mut self.build_params.profile,
			self.build_params.no_build,
		)
		.await
		{
			return display_message(&e.to_string(), false, cli);
		}
		if self.n_blocks.is_none() {
			let input = cli
				.input("How many empty blocks should be processed?")
				.default_input(&DEFAULT_N_BLOCKS.to_string())
				.required(true)
				.interact()?;
			self.n_blocks = Some(input.parse()?);
		}
		if !self.run_migrations {
			self.run_migrations = cli
				.confirm("Do you want to run pending migrations before fast-forwarding?")
				.initial_value(true)
				.interact()?;
		}
		// Prompt the user to select the source of runtime state.
		if let Err(e) = update_state_source(cli, &mut self.state) {
			return display_message(&e.to_string(), false, cli);
		};
		// Prompt the user to select the try state if no `--try-state` argument is provided.
		if self.try_state.is_none() {
			let uri = match self.state {
				Some(State::Live(LiveState { ref uri, .. })) => uri.clone(),
				_ => None,
			};
			self.try_state = Some(guide_user_to_select_try_state(cli, uri).await?);
		}

		// Test fast-forward with `try-runtime-cli` binary.
		let result = self.run(cli, user_provided_args).await;

		// Display the `fast-forward` command.
		cli.info(self.display(user_provided_args)?)?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Runtime upgrades and try-state checks completed successfully!", true, cli)
	}

	async fn run(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		user_provided_args: &[String],
	) -> anyhow::Result<()> {
		let binary_path =
			check_try_runtime_and_prompt(cli, &spinner(), self.build_params.skip_confirm).await?;
		cli.warning("NOTE: this may take some time...")?;
		let spinner = spinner();
		match self.state {
			Some(State::Live(ref live_state)) =>
				if let Some(ref uri) = live_state.uri {
					spinner.start(format!(
						"Testing fast-forward with {} blocks against live state at {}...",
						self.n_blocks.unwrap_or_default(),
						style(&uri).magenta().underlined()
					));
				},
			Some(State::Snap { ref path }) =>
				if let Some(p) = path {
					spinner.start(format!(
						"Testing fast-forward with {} blocks using a snapshot file at {}...",
						self.n_blocks.unwrap_or_default(),
						p.display()
					));
				},
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		}
		let subcommand = self.subcommand()?;
		let (command_arguments, shared_params, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

		let mut shared_args = vec![];
		collect_shared_arguments(&self.shared_params, &shared_params, &mut shared_args);

		let mut args = vec![];
		self.collect_arguments_before_subcommand(&command_arguments, &mut args)?;
		args.push(self.subcommand()?);
		collect_state_arguments(&self.state, &after_subcommand, &mut args)?;

		run_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::FastForward,
			shared_args,
			args,
			&CUSTOM_ARGS,
		)?;
		spinner.clear();
		Ok(())
	}

	fn display(&self, user_provided_args: &[String]) -> anyhow::Result<String> {
		let mut cmd_args = vec!["pop test fast-forward".to_string()];
		let mut args = vec![];
		let subcommand = self.subcommand()?;
		let (command_arguments, shared_params, after_subcommand) =
			partition_arguments(&collect_args(user_provided_args.iter().cloned()), &subcommand);

		collect_shared_arguments(&self.shared_params, &shared_params, &mut args);
		self.collect_arguments_before_subcommand(&command_arguments, &mut args)?;
		args.push(subcommand);
		collect_state_arguments(&self.state, &after_subcommand, &mut args)?;
		cmd_args.extend(args);
		Ok(cmd_args.join(" "))
	}

	// Handle arguments before the subcommand.
	fn collect_arguments_before_subcommand(
		&self,
		user_provided_args: &[String],
		args: &mut Vec<String>,
	) -> anyhow::Result<()> {
		let collected_args = collect_args(user_provided_args.iter().cloned());
		let mut c = ArgumentConstructor::new(args, &collected_args);
		if let Some(ref try_state) = self.try_state {
			c.add(&[], true, "--try-state", Some(parse_try_state_string(try_state)?));
		}
		c.add(&[], true, "--blocktime", Some(self.blocktime.to_string()));
		c.add(&[], true, "--n-blocks", self.n_blocks.map(|block| block.to_string()));
		c.add(&[], self.run_migrations, "--run-migrations", Some(String::default()));
		self.build_params.add_arguments(&mut c);
		c.finalize(&[]);
		Ok(())
	}

	fn subcommand(&self) -> anyhow::Result<String> {
		Ok(match self.state {
			Some(ref state) => StateCommand::from(state).to_string(),
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		cli::MockCli,
		common::{
			runtime::{Feature, get_mock_runtime},
			try_runtime::{
				DEFAULT_BLOCK_TIME, get_mock_snapshot, get_subcommands, get_try_state_items,
				source_try_runtime_binary,
			},
			urls,
		},
	};
	use pop_chains::{Runtime, state::LiveState};
	use pop_common::Profile;

	#[tokio::test]
	async fn fast_forward_live_state_works() -> anyhow::Result<()> {
		let mut cmd = TestFastForwardCommand::default();
		cmd.build_params.no_build = true;
		let mut cli = MockCli::new()
			.expect_intro("Performing try-state checks on simulated block execution")
			.expect_confirm(
				format!(
					"Do you want to specify which runtime to perform try-state checks on?\n{}",
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
 		.expect_info(format!("Using runtime at {}", get_mock_runtime(Some(Feature::TryRuntime)).display()))
 		.expect_input("How many empty blocks should be processed?", "10".to_string())
 		.expect_confirm("Do you want to run pending migrations before fast-forwarding?", true)
 		.expect_select(
 			"Select source of runtime state:",
 			Some(true),
 			true,
 			Some(get_subcommands()),
 			0, // live
 			None,
 		)
			.expect_input("Enter the live chain of your node:", urls::LOCAL.to_string())
			.expect_input("Enter the block hash (optional):", String::default())
			.expect_select(
				"Select state tests to execute:",
				Some(true),
				true,
				Some(get_try_state_items()),
				1,
				None,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_info(format!(
    			"pop test fast-forward --runtime={} --try-state=all --blocktime={} --n-blocks=10 --run-migrations --profile={} -n live --uri={}",
    			get_mock_runtime(Some(Feature::TryRuntime)).to_str().unwrap(),
    			DEFAULT_BLOCK_TIME,
                Profile::Debug,
                urls::LOCAL,
			));
		cmd.fast_forward(&mut cli, &[]).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn fast_forward_snapshot_works() -> anyhow::Result<()> {
		source_try_runtime_binary(&mut MockCli::new(), &spinner(), &crate::cache()?, true).await?;
		let mut cmd = TestFastForwardCommand::default();
		cmd.build_params.no_build = true;
		let mut cli = MockCli::new()
			.expect_intro("Performing try-state checks on simulated block execution")
			.expect_confirm(
				format!(
					"Do you want to specify which runtime to perform try-state checks on?\n{}",
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
 		.expect_info(format!("Using runtime at {}", get_mock_runtime(Some(Feature::TryRuntime)).display()))
 		.expect_input("How many empty blocks should be processed?", "10".to_string())
 		.expect_confirm("Do you want to run pending migrations before fast-forwarding?", true)
 		.expect_select(
 			"Select source of runtime state:",
 			Some(true),
 			true,
 			Some(get_subcommands()),
 			1, // snap
 			None,
 		)
			.expect_input(
				format!(
					"Enter path to your snapshot file?\n{}.",
					style(
						"Snapshot file can be generated using `pop test create-snapshot` command"
					)
					.dim()
				),
				get_mock_snapshot().to_str().unwrap().to_string(),
			)
			.expect_select(
				"Select state tests to execute:",
				Some(true),
				true,
				Some(get_try_state_items()),
				1,
				None,
			)
			.expect_warning("NOTE: this may take some time...")
			.expect_info(format!(
    			"pop test fast-forward --runtime={} --try-state=all --blocktime={} --n-blocks=10 --run-migrations --profile={} -n snap --path={}",
                get_mock_runtime(Some(Feature::TryRuntime)).to_str().unwrap(),
    			DEFAULT_BLOCK_TIME,
                Profile::Debug,
    			get_mock_snapshot().to_str().unwrap()
			))
			.expect_outro("Runtime upgrades and try-state checks completed successfully!");
		cmd.fast_forward(&mut cli, &[]).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn fast_forward_invalid_live_uri() -> anyhow::Result<()> {
		source_try_runtime_binary(&mut MockCli::new(), &spinner(), &crate::cache()?, true).await?;
		let mut cmd = TestFastForwardCommand {
			state: Some(State::Live(LiveState {
				uri: Some("https://example.com".to_string()),
				..Default::default()
			})),
			..Default::default()
		};
		let error = cmd.run(&mut MockCli::new(), &[]).await.unwrap_err().to_string();
		assert!(error.contains(
			r#"Failed to test with try-runtime: error: invalid value 'https://example.com' for '--uri <URI>': not a valid WS(S) url: must start with 'ws://' or 'wss://'"#,
		));
		Ok(())
	}

	#[test]
	fn display_works() -> anyhow::Result<()> {
		let mut cmd = TestFastForwardCommand {
			state: Some(State::Live(LiveState::default())),
			..Default::default()
		};
		assert_eq!(
			cmd.display(&["--blocktime=20".to_string()])?,
			"pop test fast-forward --runtime=existing --blocktime=20 live"
		);
		cmd.blocktime = DEFAULT_BLOCK_TIME;
		assert_eq!(
			cmd.display(&[])?,
			format!(
				"pop test fast-forward --runtime=existing --blocktime={} live",
				DEFAULT_BLOCK_TIME
			)
		);
		cmd.try_state = Some(TryStateSelect::Only(vec!["System".as_bytes().to_vec()]));
		assert_eq!(
			cmd.display(&[])?,
			format!(
				"pop test fast-forward --runtime=existing --try-state=System --blocktime={} live",
				DEFAULT_BLOCK_TIME
			)
		);
		assert_eq!(
			cmd.display(&["--try-state=rr-10".to_string()])?,
			format!(
				"pop test fast-forward --runtime=existing --blocktime={} --try-state=rr-10 live",
				DEFAULT_BLOCK_TIME
			)
		);
		cmd.shared_params.runtime = Runtime::Path(get_mock_runtime(Some(Feature::TryRuntime)));
		cmd.state = Some(State::Live(LiveState {
			uri: Some(urls::LOCAL.to_string()),
			..Default::default()
		}));
		assert_eq!(
			cmd.display(&[])?,
			format!(
				"pop test fast-forward --runtime={} --try-state=System --blocktime={} live --uri={}",
				get_mock_runtime(Some(Feature::TryRuntime)).display(),
				DEFAULT_BLOCK_TIME,
				urls::LOCAL
			)
		);
		assert_eq!(
			cmd.display(&[
				"--runtime".to_string(),
				get_mock_runtime(Some(Feature::TryRuntime)).to_str().unwrap().to_string(),
				"--try-state".to_string(),
				"all".to_string(),
				"live".to_string(),
			])?,
			format!(
				"pop test fast-forward --runtime={} --blocktime={} --try-state=all live --uri={}",
				get_mock_runtime(Some(Feature::TryRuntime)).display(),
				DEFAULT_BLOCK_TIME,
				urls::LOCAL
			)
		);
		Ok(())
	}

	#[test]
	fn collect_arguments_before_subcommand_works() -> anyhow::Result<()> {
		let test_cases: Vec<(&str, Box<dyn Fn(&mut TestFastForwardCommand)>, &str)> = vec![
			(
				"--n-blocks=20",
				Box::new(|cmd| {
					cmd.n_blocks = Some(10);
				}),
				"--n-blocks=10",
			),
			(
				"--blocktime=20",
				Box::new(|cmd| {
					cmd.blocktime = 10;
				}),
				"--blocktime=10",
			),
			(
				"--try-state=all",
				Box::new(|cmd| {
					cmd.try_state = Some(TryStateSelect::RoundRobin(10));
				}),
				"--try-state=rr-10",
			),
			(
				"--run-migrations",
				Box::new(|cmd| {
					cmd.run_migrations = true;
				}),
				"--run-migrations",
			),
			(
				"-y",
				Box::new(|cmd| {
					cmd.build_params.skip_confirm = true;
				}),
				"-y",
			),
			(
				"--skip-confirm",
				Box::new(|cmd| {
					cmd.build_params.skip_confirm = true;
				}),
				"-y",
			),
		];
		for (provided_arg, update_fn, expected_arg) in test_cases {
			let mut command = TestFastForwardCommand::default();
			let mut args = vec![];
			// Keep the user-provided argument unchanged.
			command.collect_arguments_before_subcommand(&[provided_arg.to_string()], &mut args)?;
			assert_eq!(args.iter().filter(|a| a.contains(&provided_arg.to_string())).count(), 1);

			// If there exists an argument with the same name as the provided argument, skip it.
			command.collect_arguments_before_subcommand(&[], &mut args)?;
			assert_eq!(args.iter().filter(|a| a.contains(&provided_arg.to_string())).count(), 1);

			// If the user does not provide an argument, modify with the argument updated during
			// runtime.
			let mut args = vec![];
			update_fn(&mut command);
			command.collect_arguments_before_subcommand(&[], &mut args)?;
			assert_eq!(args.iter().filter(|a| a.contains(&expected_arg.to_string())).count(), 1);
		}
		Ok(())
	}

	#[test]
	fn subcommand_works() -> anyhow::Result<()> {
		let mut command = TestFastForwardCommand {
			state: Some(State::Live(LiveState::default())),
			..Default::default()
		};
		assert_eq!(command.subcommand()?, String::from("live"));
		command.state = Some(State::Snap { path: None });
		assert_eq!(command.subcommand()?, String::from("snap"));
		Ok(())
	}
}
