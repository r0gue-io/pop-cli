#[cfg(feature = "contract")]
mod contract;
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
	/// Deploy a parachain to a local network, powered by zombienet-sdk <https://github.com/paritytech/zombienet-sdk>.
	#[clap(alias = "p")]
	Parachain(parachain::ZombienetCommand),
	#[cfg(feature = "contract")]
	/// Deploy a smart contract to a node.
	#[clap(alias = "c")]
	Contract(contract::UpContractCommand),
}
