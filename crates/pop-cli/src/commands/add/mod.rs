// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

pub mod config_type;
pub mod runtime_pallet;

/// Arguments for adding a new feature to existing code
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct AddArgs {
	#[command(subcommand)]
	pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
	/// Add a new config type to an existing pallet
	#[cfg(feature = "parachain")]
	#[clap(alias = "C")]
	ConfigType(config_type::AddConfigTypeCommand),
	/// Add a new pallet to an existing runtime
	#[clap(alias = "P")]
	RuntimePallet(runtime_pallet::AddRuntimePalletCommand),
}
