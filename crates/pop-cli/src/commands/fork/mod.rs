// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use std::fmt::{Display, Formatter, Result};
use url::Url;

#[cfg(feature = "chain")]
pub(crate) mod chain;
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub(crate) mod contract;

/// Arguments of the fork command.
#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub(crate) struct ForkArgs {
	/// Entrypoint of the fork command.
	#[command(subcommand)]
	pub(crate) command: Command,
	/// Websocket endpoint of a node.
	#[arg(short, long, value_parser)]
	pub(crate) endpoint: Url,
}

/// Launch a local fork of a live running chain or contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Create a local fork of live running chains.
	#[cfg(feature = "chain")]
	Chain(chain::ForkChainCommand),
	/// Create a local fork of live contracts.
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	Contract(contract::ForkContractCommand),
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(feature = "chain")]
			Command::Chain(_) => write!(f, "chain"),
			#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
			Command::Contract(_) => write!(f, "contract"),
		}
	}
}
