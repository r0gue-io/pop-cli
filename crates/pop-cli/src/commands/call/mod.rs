// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use std::fmt::{Display, Formatter, Result};
#[cfg(feature = "chain")]
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
	#[cfg(feature = "chain")]
	#[clap(aliases = ["C", "p", "parachain"])]
	Chain(chain::CallChainCommand),
	/// Call a contract
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	#[clap(alias = "c")]
	Contract(contract::CallContractCommand),
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn command_display_works() {
		#[cfg(feature = "chain")]
		assert_eq!(Command::Chain(Default::default()).to_string(), "chain");
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}
}
