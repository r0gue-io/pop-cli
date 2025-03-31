use crate::cli;
use clap::Args;
use pop_parachains::{parse, state::State};

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
	try_state: frame_try_runtime::TryStateSelect,

	/// The ws uri from which to fetch the block.
	///
	/// This will always fetch the next block of whatever `state` is referring to, because this is
	/// the only sensible combination. In other words, if you have the state of block `n`, you
	/// should execute block `n+1` on top of it.
	///
	/// If `state` is `Live`, this can be ignored and the same uri is used for both.
	#[arg(
		long,
		value_parser = parse::url
	)]
	block_ws_uri: Option<String>,

	/// The state type to use.
	#[command(subcommand)]
	state: Option<State>,
}

impl TestExecuteBlockCommand {
	pub(crate) async fn execute(mut self, cli: &mut impl cli::traits::Cli) -> anyhow::Result<()> {
		Ok(())
	}
}
