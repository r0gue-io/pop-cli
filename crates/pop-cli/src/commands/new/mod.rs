// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "parachain")]
pub mod pallet;
#[cfg(feature = "parachain")]
pub mod parachain;

/// Arguments for generating a new project.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct NewArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Generate a new parachain, pallet or smart contract.
#[derive(Subcommand)]
pub enum Command {
	/// Generate a new parachain
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Parachain(parachain::NewParachainCommand),
	/// Generate a new pallet
	#[cfg(feature = "parachain")]
	#[clap(alias = "P")]
	Pallet(pallet::NewPalletCommand),
	/// Generate a new smart contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::NewContractCommand),
}
