use clap::{Args, Subcommand};

#[cfg(feature = "parachain")]
pub mod pallet;
#[cfg(feature = "parachain")]
pub mod parachain;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct NewArgs {
    #[command(subcommand)]
    pub command: NewCommands,
}

#[derive(Subcommand)]
pub enum NewCommands {
    /// Generate a new parachain template
    #[cfg(feature = "parachain")]
    Parachain(parachain::NewParachainCommand),
    /// Generate a new pallet template
    #[cfg(feature = "parachain")]
    Pallet(pallet::NewPalletCommand),
}
