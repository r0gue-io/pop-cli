use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub(crate) mod contract;
#[cfg(feature = "parachain")]
pub(crate) mod parachain;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct BuildArgs {
	#[command(subcommand)]
	pub command: BuildCommands,
}

#[derive(Subcommand)]
pub(crate) enum BuildCommands {
	/// Build a parachain
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Parachain(parachain::BuildParachainCommand),
	/// Build a contract, generate metadata, bundle together in a `<name>.contract` file
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::BuildContractCommand),
}
