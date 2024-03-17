use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub mod contract;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct TestArgs {
	#[command(subcommand)]
	pub command: TestCommands,
}

#[derive(Subcommand)]
pub(crate) enum TestCommands {
	/// Test a smart contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::TestContractCommand),
}
