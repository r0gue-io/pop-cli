// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};

#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "parachain")]
pub mod pallet;
#[cfg(feature = "parachain")]
pub mod parachain;

#[macro_export]
macro_rules! enum_variants {
	($e: ty) => {{
		PossibleValuesParser::new(
			<$e>::VARIANTS
				.iter()
				.map(|p| PossibleValue::new(p.as_ref()))
				.collect::<Vec<_>>(),
		)
		.try_map(|s| {
			<$e>::from_str(&s).map_err(|e| format!("could not convert from {s} to provider"))
		})
	}};
}

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct NewArgs {
	#[command(subcommand)]
	pub command: NewCommands,
}

#[derive(Subcommand)]
pub enum NewCommands {
	/// Generate a new parachain
	#[cfg(feature = "parachain")]
	#[clap(alias = "p")]
	Parachain(parachain::NewParachainCommand),
	/// Generate a new pallet
	#[cfg(feature = "parachain")]
	#[clap(alias = "m")] // (m)odule, as p used above
	Pallet(pallet::NewPalletCommand),
	/// Generate a new smart contract
	#[cfg(feature = "contract")]
	#[clap(alias = "c")]
	Contract(contract::NewContractCommand),
}
