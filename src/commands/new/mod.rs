use clap::{Args, Subcommand};

pub mod pallet;
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
    Parachain(parachain::NewParachainCommand),
     /// Generate a new pallet template
    Pallet(pallet::NewPalletCommand),
}