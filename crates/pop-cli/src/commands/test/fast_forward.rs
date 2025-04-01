use std::{thread::sleep, time::Duration};

use crate::{
	cli::{
		self,
		traits::{Confirm, Input},
	},
	common::{
		prompt::display_message,
		try_runtime::{
			check_try_runtime_and_prompt, collect_state_arguments, partition_arguments,
			update_state_source, ArgumentConstructor, DEFAULT_BLOCK_TIME,
		},
	},
};
use clap::Args;
use cliclack::spinner;
use console::style;
use frame_try_runtime::TryStateSelect;
use pop_parachains::{
	run_try_runtime,
	state::{State, StateCommand},
	TryRuntimeCliCommand,
};

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

impl TestFastForwardCommand {
	pub(crate) async fn execute(mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		cli.intro("Testing fast-forward...")?;
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

		// Test fast-forward with `try-runtime-cli` binary.
		let result = self.run(cli).await;

		// Display the `fast-forward` command.
		cli.info(self.display()?)?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		display_message("Tested fast-forwarding successfully!", true, cli)
	}

	async fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
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
						"Running migrations using a snapshot file at {}...",
						p.display()
					));
				},
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		}
		sleep(Duration::from_secs(1));

		let subcommand = self.subcommand()?;
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let (command_arguments, _, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

		let mut args = vec![];
		self.collect_arguments_before_subcommand(&command_arguments, &mut args);
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

	fn display(&self) -> anyhow::Result<String> {
		let mut cmd_args = vec!["pop test fast-forward".to_string()];
		let mut args = vec![];
		let subcommand = self.subcommand()?;
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let (command_arguments, _, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

		self.collect_arguments_before_subcommand(&command_arguments, &mut args);
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
	) {
		let mut c = ArgumentConstructor::new(args, user_provided_args);
		c.add(&[], true, "--blocktime", Some(self.blocktime.to_string()));
		c.add(&["--n-blocks"], true, "--n-blocks", self.n_blocks.map(|block| block.to_string()));
		c.add(
			&["--run-migrations"],
			self.run_migrations,
			"--run-migrations",
			Some(String::default()),
		);
		// These are custom arguments not used in `try-runtime-cli`.
		c.add(&["--skip-confirm"], self.skip_confirm, "-y", Some(String::default()));
		c.finalize(&[]);
	}

	fn subcommand(&self) -> anyhow::Result<String> {
		Ok(match self.state {
			Some(ref state) => StateCommand::from(state).to_string(),
			None => return Err(anyhow::anyhow!("No subcommand provided")),
		})
	}
}
