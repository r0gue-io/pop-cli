// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use std::fmt::{Display, Formatter, Result};
#[cfg(feature = "parachain")]
pub(crate) mod chain;
#[cfg(feature = "contract")]
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
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::CallContractCommand),
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(feature = "parachain")]
			Command::Chain(_) => write!(f, "chain"),
			#[cfg(feature = "contract")]
			Command::Contract(_) => write!(f, "contract"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn command_display_works() {
		assert_eq!(Command::Chain(Default::default()).to_string(), "chain");
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}
}
