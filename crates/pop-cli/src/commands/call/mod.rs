// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use std::fmt::{Display, Formatter, Result};
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
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

/// Call a parachain or a smart contract.
#[derive(Subcommand)]
pub(crate) enum Command {
	/// Call a parachain
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Parachain(parachain::CallParachainCommand),
	/// Call a contract
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	#[clap(alias = "c")]
	Contract(contract::CallContractCommand),
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(feature = "parachain")]
			Command::Parachain(_) => write!(f, "parachain"),
			#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
			Command::Contract(_) => write!(f, "contract"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn command_display_works() {
		#[cfg(feature = "parachain")]
		assert_eq!(Command::Chain(Default::default()).to_string(), "chain");
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}
}
