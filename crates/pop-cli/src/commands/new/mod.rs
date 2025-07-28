// SPDX-License-Identifier: GPL-3.0

use clap::{Args, Subcommand};
use std::fmt::{Display, Formatter, Result};
use crate::cli::{traits::Cli as _, Cli};
use anyhow::Result as AnyhowResult;

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
	pub command: Option<Command>,
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

/// Guide the user to select what type of project to create
pub async fn guide_user_to_select_command() -> AnyhowResult<Command> {
	Cli.intro("🚀 Welcome to Pop CLI!")?;
	
	let mut prompt = cliclack::select("What would you like to create?".to_string());
	
	// Add available options based on features
	#[cfg(feature = "parachain")]
	{
		prompt = prompt.item(
			"parachain", 
			"🌐 Parachain", 
			"Build your own blockchain with custom logic, token economics, and governance"
		);
		prompt = prompt.item(
			"pallet", 
			"🧩 Pallet", 
			"Create reusable blockchain modules to add features like tokens, voting, or custom logic"
		);
	}
	
	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	{
		prompt = prompt.item(
			"contract", 
			"📝 Smart Contract", 
			"Write decentralized applications with ink! for the Polkadot ecosystem"
		);
	}
	
	// Set initial selection to the first available option
	#[cfg(feature = "parachain")]
	{
		prompt = prompt.initial_value("parachain");
	}
	#[cfg(all(
		any(feature = "polkavm-contracts", feature = "wasm-contracts"),
		not(feature = "parachain")
	))]
	{
		prompt = prompt.initial_value("contract");
	}
	
	let selection = prompt.interact()?;
	
	match selection {
		#[cfg(feature = "parachain")]
		"parachain" => Ok(Command::Parachain(parachain::NewParachainCommand {
			name: None,
			provider: None,
			template: None,
			release_tag: None,
			symbol: None,
			decimals: None,
			initial_endowment: None,
			verify: false,
		})),
		#[cfg(feature = "parachain")]
		"pallet" => Ok(Command::Pallet(pallet::NewPalletCommand {
			name: None,
			authors: None,
			description: None,
			mode: None,
		})),
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		"contract" => Ok(Command::Contract(contract::NewContractCommand {
			name: None,
			contract_type: None,
			template: None,
		})),
		_ => Err(anyhow::anyhow!("Invalid selection")),
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
