use clap::{Args, Subcommand};

pub(crate) mod contract;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CallArgs {
	#[command(subcommand)]
	pub command: CallCommands,
}

#[derive(Subcommand)]
pub(crate) enum CallCommands {
	/// Call a contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::CallContractCommand),
}
