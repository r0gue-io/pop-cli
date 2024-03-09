use clap::{Args, Subcommand};

pub mod pallet;

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct AddArgs {
	#[command(subcommand)]
	pub command: AddCommands,
}

#[derive(Subcommand)]
pub(crate) enum AddCommands {
	/// Add a pallet to a runtime
	Pallet(pallet::AddPalletCommand),
}