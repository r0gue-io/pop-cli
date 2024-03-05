#[cfg(feature = "parachain")]
mod parachain;
#[cfg(feature = "contract")]
mod contract;

use clap::{Args, Subcommand};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpArgs {
    #[command(subcommand)]
    pub(crate) command: UpCommands,
}

#[derive(Subcommand)]
pub(crate) enum UpCommands {
    #[cfg(feature = "parachain")]
    /// Deploy a parachain to a network.
    Parachain(parachain::ZombienetCommand),
    #[cfg(feature = "contract")]
    /// Deploy a smart contract to a network.
    Contract(contract::UpContractCommand),
}
