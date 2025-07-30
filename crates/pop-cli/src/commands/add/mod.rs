// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

pub mod pallet;

/// Arguments for adding a new feature to existing code
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct AddArgs {
	#[command(subcommand)]
	pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
	/// Add a new pallet to an existing runtime
	#[clap(alias = "P")]
	Pallet(pallet::AddPalletCommand),
}
