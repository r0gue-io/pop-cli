use crate::{
	cli::{self, traits::Input},
	common::{
		prompt::display_message,
		try_runtime::{
			check_try_runtime_and_prompt, collect_state_arguments, partition_arguments,
			update_state_source, ArgumentConstructor,
		},
	},
};
use clap::Args;
use cliclack::spinner;
use frame_try_runtime::TryStateSelect;
use pop_parachains::{
	parse, parse_try_state_string, run_try_runtime,
	state::{State, StateCommand},
	TryRuntimeCliCommand,
};

const CUSTOM_ARGS: [&str; 2] = ["--skip-confirm", "-y"];

#[derive(Args)]
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

	/// The websocket URI from which to fetch the block.
	///
	/// This will always fetch the next block of whatever `state` is referring to, because this is
	/// the only sensible combination. In other words, if you have the state of block `n`, you
	/// should execute block `n+1` on top of it.
	#[clap(
		long,
		value_parser = parse::url,
	)]
	block_ws_uri: Option<String>,

	/// The state to use.
	#[command(subcommand)]
	state: Option<State>,

	/// Automatically source the needed binary required without prompting for confirmation.
	#[clap(short = 'y', long)]
	skip_confirm: bool,
}

impl TestExecuteBlockCommand {
	pub(crate) async fn execute(mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		// Throw error if block URI is provided with `live` state.
		if self.block_ws_uri.is_some() {
			if let Some(State::Live(..)) = self.state {
				return display_message(
					"Block URI must not be provided when state is `live`.",
					false,
					cli,
				)
			};
		}
		// Prompt the user to select the source of runtime state.
		if let Err(e) = update_state_source(cli, &mut self.state) {
			return display_message(&e.to_string(), false, cli);
		};
		// Prompt the user to input the block URI if state is `snap`.
		if self.block_ws_uri.is_none() {
			if let Some(State::Snap { .. }) = self.state {
				let input = cli.input("Enter the URI to fetch the block from:").interact()?;
				self.block_ws_uri = Some(parse::url(&input)?);
			}
		}

		// Test block execution with `try-runtime-cli` binary.
		let result = self.run(cli).await;

		// Display the `execute-block` command.
		cli.info(self.display()?)?;
		if let Err(e) = result {
			return display_message(&e.to_string(), false, cli);
		}
		return display_message("Tested block execution successfully!", true, cli);
	}

	async fn run(&mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		let binary_path = check_try_runtime_and_prompt(cli, self.skip_confirm).await?;
		cli.warning("NOTE: this may take some time...")?;
		let spinner = spinner();
		spinner.start("Executing block...");

		let subcommand = self.subcommand()?;
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
		let (command_arguments, _, after_subcommand) =
			partition_arguments(user_provided_args, &subcommand);

		let mut args = vec![];
		self.collect_arguments_before_subcommand(&command_arguments, &mut args)?;
		args.push(self.subcommand()?);
		collect_state_arguments(&self.state, &after_subcommand, &mut args)?;

		run_try_runtime(
			&binary_path,
			TryRuntimeCliCommand::ExecuteBlock,
			vec![],
			args,
			&CUSTOM_ARGS,
		)?;
		spinner.stop("");
		Ok(())
	}

	fn display(&self) -> anyhow::Result<String> {
		let mut cmd_args = vec!["pop test execute-block".to_string()];
		let mut args = vec![];
		let subcommand = self.subcommand()?;
		let user_provided_args: Vec<String> = std::env::args().skip(3).collect();
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
		c.add(&[], true, "--block-ws-uri", self.block_ws_uri.clone());
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
