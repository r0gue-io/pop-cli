use clap::{Args, Subcommand};

pub mod contract;
pub mod parachain;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct BuildArgs {
    #[command(subcommand)]
    pub command: BuildCommands,
}

#[derive(Subcommand)]
pub(crate) enum BuildCommands {
    /// Build a parachain template
    #[cfg(feature = "parachain")]
    #[clap(alias = "p")]
    Parachain(parachain::BuildParachainCommand),
    /// Compiles the contract, generates metadata, bundles both together in a
     /// `<name>.contract` file
    #[cfg(feature = "contract")]
    #[clap(alias = "c")]
    Contract(contract::BuildContractCommand),
}
