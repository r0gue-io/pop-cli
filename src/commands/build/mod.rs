use clap::{Args, Subcommand};

pub mod contract;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct BuildArgs {
    #[command(subcommand)]
    pub command: BuildCommands,
}

#[derive(Subcommand)]
pub enum BuildCommands {
    /// Compiles the contract, generates metadata, bundles both together in a
    /// `<name>.contract` file
    #[cfg(feature = "contract")]
    Contract(contract::BuildContractCommand),
}