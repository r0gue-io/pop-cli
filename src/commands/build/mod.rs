use clap::{Args, Subcommand};

pub mod contract;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct BuildArgs {
    #[command(subcommand)]
    pub command: BuildCommands,
}

#[derive(Subcommand)]
pub(crate) enum BuildCommands {
    /// Compiles the contract, generates metadata, bundles both together in a
    /// `<name>.contract` file
    #[cfg(feature = "contract")]
    #[clap(alias = "c")]
    Contract(contract::BuildContractCommand),
}
