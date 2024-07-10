// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
mod contract;
#[cfg(feature = "parachain")]
mod parachain;

/// Arguments for launching or deploying.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct UpArgs {
	#[command(subcommand)]
	pub(crate) command: Command,
}

/// Launch a local network or deploy a smart contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	#[cfg(feature = "parachain")]
	/// Launch a local network.
	#[clap(alias = "p")]
	Parachain(parachain::ZombienetCommand),
	#[cfg(feature = "contract")]
	/// Deploy a smart contract.
	#[clap(alias = "c")]
	Contract(contract::UpContractCommand),
}
