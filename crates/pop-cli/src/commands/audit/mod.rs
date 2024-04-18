use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub mod contract;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct AuditArgs {
	#[command(subcommand)]
	pub command: AuditCommands,
}

#[derive(Subcommand)]
pub(crate) enum AuditCommands {
	/// Audit a smart contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::AuditContractCommand),
}
