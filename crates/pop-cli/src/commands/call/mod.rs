// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

#[cfg(feature = "parachain")]
pub(crate) mod chain;
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod contract;

/// Arguments for calling a smart contract.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct CallArgs {
	#[command(subcommand)]
	pub command: Command,
}

/// Call a chain or a smart contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Call a chain
	#[cfg(feature = "parachain")]
	#[clap(alias = "p", visible_aliases = ["parachain"])]
	Chain(chain::CallChainCommand),
	/// Call a contract
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	#[clap(alias = "c")]
	Contract(contract::CallContractCommand),
}
