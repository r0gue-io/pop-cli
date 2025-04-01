use crate::{
	cli::{
		self,
		traits::{Confirm, Input},
	},
	common::{
		prompt::display_message,
		try_runtime::{
			argument_exists, check_try_runtime_and_prompt, collect_state_arguments,
			guide_user_to_select_try_state, partition_arguments, update_state_source,
			ArgumentConstructor, DEFAULT_BLOCK_TIME,
		},
	},
};
use clap::Args;
use cliclack::spinner;
use console::style;
use frame_try_runtime::TryStateSelect;
use pop_parachains::{
	parse_try_state_string, run_try_runtime,
	state::{State, StateCommand},
	TryRuntimeCliCommand,
};
use std::{thread::sleep, time::Duration};

const CUSTOM_ARGS: [&str; 2] = ["--skip-confirm", "-y"];
const DEFAULT_N_BLOCKS: u64 = 10;

#[derive(Args)]
pub(crate) struct TestFastForwardCommand {
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
	#[arg(long, default_value = "all")]
	try_state: TryStateSelect,

	/// Whether to run pending migrations before fast-forwarding.
	#[arg(long)]
	run_migrations: bool,

	/// The state type to use.
	#[command(subcommand)]
	state: Option<State>,

	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl Default for TestFastForwardCommand {
	fn default() -> Self {
		Self {
			n_blocks: None,
			blocktime: DEFAULT_BLOCK_TIME,
			try_state: TryStateSelect::All,
			run_migrations: false,
			state: None,
			skip_confirm: false,
		}
	}
}

impl TestFastForwardCommand {
	pub(crate) async fn execute(mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Testing fast-forward...")?;
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
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
		if !argument_exists(&user_provided_args, "--try-state") {
			self.try_state = guide_user_to_select_try_state(cli)?;
		}

		// Test fast-forward with `try-runtime-cli` binary.
		let result = self.run(cli, user_provided_args.clone()).await;

		// Display the `fast-forward` command.
		cli.info(self.display(user_provided_args)?)?;
		if let Err(e) = result {
			println!("error: {}", e);
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Tested fast-forwarding successfully!", true, cli)
	}

	async fn run(
		&mut self,
		cli: &mut impl cli::traits::Cli,
		user_provided_args: Vec<String>,
	) -> anyhow::Result<()> {
		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		cli.warning("NOTE: this may take some time...")?;
		let spinner = spinner();
		match self.state {
			Some(State::Live(ref live_state)) => {
				if let Some(ref uri) = live_state.uri {
					spinner.start(format!(
						"Testing fast-forward with {} blocks against live state at {}...",
						self.n_blocks.unwrap_or_default(),
						style(&uri).magenta().underlined()
					));
				}
			},
			Some(State::Snap { ref path }) => {
				if let Some(p) = path {
					spinner.start(format!(
						"Testing fast-forward with {} blocks using a snapshot file at {}...",
						self.n_blocks.unwrap_or_default(),
						p.display()
					));
				}
			},
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		}
		sleep(Duration::from_secs(1));

		let subcommand = self.subcommand()?;
		let (command_arguments, _, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

		let mut args = vec![];
		self.collect_arguments_before_subcommand(&command_arguments, &mut args)?;
		args.push(self.subcommand()?);
		collect_state_arguments(&self.state, &after_subcommand, &mut args)?;

		run_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::FastForward,
			vec![],
			args,
			&CUSTOM_ARGS,
		)?;
		spinner.stop("");
		Ok(())
	}

	fn display(&self, user_provided_args: Vec<String>) -> anyhow::Result<String> {
		let mut cmd_args = vec!["pop test fast-forward".to_string()];
		let mut args = vec![];
		let subcommand = self.subcommand()?;
		let (command_arguments, _, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

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
		let mut c = ArgumentConstructor::new(args, user_provided_args);
		c.add(&[], true, "--try-state", Some(parse_try_state_string(self.try_state.clone())?));
		c.add(&[], true, "--blocktime", Some(self.blocktime.to_string()));
		c.add(&[], true, "--n-blocks", self.n_blocks.map(|block| block.to_string()));
		c.add(&[], self.run_migrations, "--run-migrations", Some(String::default()));
		// These are custom arguments not used in `try-runtime-cli`.
		c.add(&["--skip-confirm"], self.skip_confirm, "-y", Some(String::default()));
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
		common::try_runtime::{
			get_mock_snapshot, get_subcommands, get_try_state_items, source_try_runtime_binary,
			DEFAULT_BLOCK_TIME, DEFAULT_LIVE_NODE_URL,
		},
	};
	use pop_parachains::state::LiveState;

	#[tokio::test]
	async fn test_fast_forward_live_state_works() -> anyhow::Result<()> {
		let cmd = TestFastForwardCommand::default();
		let mut cli = MockCli::new()
			.expect_intro("Testing fast-forward...")
			.expect_input("How many empty blocks should be processed?", "10".to_string())
			.expect_confirm("Do you want to run pending migrations before fast-forwarding?", true)
			.expect_select(
				"Select source of runtime state to run the migration with:",
				Some(true),
				true,
				Some(get_subcommands()),
				0, // live
				None,
			)
			.expect_input("Enter the live chain of your node:", DEFAULT_LIVE_NODE_URL.to_string())
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
    			"pop test fast-forward --try-state=all --blocktime={} --n-blocks=10 --run-migrations live --uri={}",
    			DEFAULT_BLOCK_TIME,
                DEFAULT_LIVE_NODE_URL,
			))
			.expect_outro("Tested fast-forwarding successfully!");
		cmd.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn test_fast_forward_snapshot_works() -> anyhow::Result<()> {
		let cmd = TestFastForwardCommand::default();
		let mut cli = MockCli::new()
			.expect_intro("Testing fast-forward...")
			.expect_input("How many empty blocks should be processed?", "10".to_string())
			.expect_confirm("Do you want to run pending migrations before fast-forwarding?", true)
			.expect_select(
				"Select source of runtime state to run the migration with:",
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
    			"pop test fast-forward --try-state=all --blocktime={} --n-blocks=10 --run-migrations snap --path={}",
    			DEFAULT_BLOCK_TIME,
    			get_mock_snapshot().to_str().unwrap()
			))
			.expect_outro("Tested fast-forwarding successfully!");
		cmd.execute(&mut cli).await?;
		cli.verify()
	}

	#[tokio::test]
	async fn test_on_runtime_upgrade_invalid_live_uri() -> anyhow::Result<()> {
		source_try_runtime_binary(&mut MockCli::new(), &crate::cache()?, true).await?;
		let mut cmd = TestFastForwardCommand::default();
		cmd.state = Some(State::Live(LiveState {
			uri: Some("https://example.com".to_string()),
			..Default::default()
		}));
		let error = cmd.run(&mut MockCli::new(), vec![]).await.unwrap_err().to_string();
		assert!(error.contains(
			r#"Failed to test with try-runtime: error: invalid value 'https://example.com' for '--uri <URI>': not a valid WS(S) url: must start with 'ws://' or 'wss://'"#,
		));
		Ok(())
	}

	#[test]
	fn display_works() -> anyhow::Result<()> {
		let mut cmd = TestFastForwardCommand::default();
		cmd.state = Some(State::Live(LiveState::default()));
		assert_eq!(
			cmd.display(vec!["--blocktime=20".to_string()])?,
			"pop test fast-forward --try-state=all --blocktime=20 live"
		);
		cmd.blocktime = DEFAULT_BLOCK_TIME;
		assert_eq!(
			cmd.display(vec![])?,
			format!(
				"pop test fast-forward --try-state=all --blocktime={} live",
				DEFAULT_BLOCK_TIME
			)
		);
		cmd.try_state = TryStateSelect::Only(vec!["System".as_bytes().to_vec()]);
		assert_eq!(
			cmd.display(vec![])?,
			format!(
				"pop test fast-forward --try-state=System --blocktime={} live",
				DEFAULT_BLOCK_TIME
			)
		);
		assert_eq!(
			cmd.display(vec!["--try-state=rr-10".to_string()])?,
			format!(
				"pop test fast-forward --blocktime={} --try-state=rr-10 live",
				DEFAULT_BLOCK_TIME
			)
		);
		cmd.state = Some(State::Live(LiveState {
			uri: Some(DEFAULT_LIVE_NODE_URL.to_string()),
			..Default::default()
		}));
		assert_eq!(
			cmd.display(vec![])?,
			format!(
				"pop test fast-forward --try-state=System --blocktime={} live --uri={}",
				DEFAULT_BLOCK_TIME, DEFAULT_LIVE_NODE_URL
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
					cmd.try_state = TryStateSelect::RoundRobin(10);
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
					cmd.skip_confirm = true;
				}),
				"-y",
			),
			(
				"--skip-confirm",
				Box::new(|cmd| {
					cmd.skip_confirm = true;
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
		let mut command = TestFastForwardCommand::default();
		command.state = Some(State::Live(LiveState::default()));
		assert_eq!(command.subcommand()?, String::from("live"));
		command.state = Some(State::Snap { path: None });
		assert_eq!(command.subcommand()?, String::from("snap"));
		Ok(())
	}
}
