// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use std::fmt::{Display, Formatter, Result};

#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub mod contract;
#[cfg(feature = "parachain")]
pub mod pallet;
#[cfg(feature = "parachain")]
pub mod parachain;

/// The possible values from the variants of an enum.
#[macro_export]
macro_rules! enum_variants {
	($e: ty) => {{
		PossibleValuesParser::new(
			<$e>::VARIANTS
				.iter()
				.map(|p| PossibleValue::new(p.as_ref()))
				.collect::<Vec<_>>(),
		)
		.try_map(|s| <$e>::from_str(&s).map_err(|e| format!("could not convert from {s} to type")))
	}};
}

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
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	#[clap(alias = "c")]
	Contract(contract::NewContractCommand),
}

impl Display for Command {
	fn fmt(&self, f: &mut Formatter<'_>) -> Result {
		match self {
			#[cfg(feature = "parachain")]
			Command::Parachain(_) => write!(f, "chain"),
			#[cfg(feature = "parachain")]
			Command::Pallet(_) => write!(f, "pallet"),
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
		assert_eq!(Command::Parachain(Default::default()).to_string(), "chain");
		#[cfg(feature = "parachain")]
		assert_eq!(Command::Pallet(Default::default()).to_string(), "pallet");
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}
}
