// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub(crate) mod contract;
#[cfg(feature = "parachain")]
pub(crate) mod parachain;

/// Arguments for calling a smart contract.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CallArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Call a smart contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Call a parachain
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Parachain(parachain::CallParachainCommand),
	/// Call a contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::CallContractCommand),
}
