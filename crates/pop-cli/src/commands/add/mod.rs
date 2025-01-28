// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

pub mod pallet_config_type;
pub mod runtime_pallet;

/// Arguments for adding a new feature to existing code
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct AddArgs {
	#[command(subcommand)]
	pub command: Command,
}

#[derive(Subcommand)]
pub enum Command{
    /// Expand your runtime using Pop-Cli
#[command(subcommand)]
    Runtime(RuntimeCommand),
    /// Expand a pallet using Pop-Cli
    #[command(subcommand)]
    Pallet(PalletCommand)
}

#[derive(Subcommand)]
pub enum RuntimeCommand {
	/// Add pallets to an existing runtime
	Pallet(runtime_pallet::AddPalletCommand),
}

#[derive(Subcommand)]
pub enum PalletCommand {
	/// Add a new config type to an existing pallet
	ConfigType(pallet_config_type::AddConfigTypeCommand),
}
