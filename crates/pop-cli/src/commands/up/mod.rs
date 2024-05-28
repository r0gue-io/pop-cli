// SPDX-License-Identifier: GPL-3.0

#[cfg(feature = "contract")]
mod contract;
#[cfg(feature = "contract")]
mod contracts_node;
#[cfg(feature = "parachain")]
mod parachain;

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
	/// Launch a local network.
	#[clap(alias = "p")]
	Parachain(parachain::ZombienetCommand),
	#[cfg(feature = "contract")]
	/// Deploy a smart contract to a node.
	#[clap(alias = "c")]
	Contract(contract::UpContractCommand),
	#[cfg(feature = "contract")]
	/// Deploy a contracts node.
	#[clap(alias = "n")]
	ContractsNode(contracts_node::ContractsNodeCommand),
}
