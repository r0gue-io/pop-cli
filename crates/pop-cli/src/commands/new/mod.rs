// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::Result as AnyhowResult;
use clap::{Args, Subcommand};
use std::fmt::{Display, Formatter, Result};

#[cfg(feature = "chain")]
pub mod chain;
#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
pub mod contract;
#[cfg(feature = "chain")]
pub mod pallet;

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
	#[cfg(feature = "chain")]
	#[clap(aliases = ["C", "p", "parachain"])]
	Chain(chain::NewChainCommand),
	/// Generate a new pallet
	#[cfg(feature = "chain")]
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
			#[cfg(feature = "chain")]
			Command::Chain(_) => write!(f, "chain"),
			#[cfg(feature = "chain")]
			Command::Pallet(_) => write!(f, "pallet"),
			#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
			Command::Contract(_) => write!(f, "contract"),
		}
	}
}

/// Guide the user to select what type of project to create
pub fn guide_user_to_select_command(cli: &mut impl cli::traits::Cli) -> AnyhowResult<Command> {
	cli.intro("Welcome to Pop CLI!")?;

	let mut prompt = cli.select("What would you like to create?".to_string());

	// Add available options based on features
	#[cfg(feature = "chain")]
	{
		prompt = prompt.item("chain", "Chain", "Build your own custom chain");
		prompt = prompt.item("pallet", "Pallet", "Create reusable and customizable chain modules");
	}

	#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
	{
		prompt = prompt.item("contract", "Smart Contract", "Write ink! smart contracts");
	}

	// Set initial selection to the first available option
	#[cfg(feature = "chain")]
	{
		prompt = prompt.initial_value("chain");
	}
	#[cfg(all(
		any(feature = "polkavm-contracts", feature = "wasm-contracts"),
		not(feature = "chain")
	))]
	{
		prompt = prompt.initial_value("contract");
	}

	let selection = prompt.interact()?;

	match selection {
		#[cfg(feature = "chain")]
		"chain" => Ok(Command::Chain(chain::NewChainCommand {
			name: None,
			provider: None,
			template: None,
			release_tag: None,
			symbol: None,
			decimals: None,
			initial_endowment: None,
			verify: false,
		})),
		#[cfg(feature = "chain")]
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
	use crate::cli::MockCli;

	#[test]
	fn command_display_works() {
		#[cfg(feature = "chain")]
		assert_eq!(Command::Chain(Default::default()).to_string(), "chain");
		#[cfg(feature = "chain")]
		assert_eq!(Command::Pallet(Default::default()).to_string(), "pallet");
		#[cfg(any(feature = "polkavm-contracts", feature = "wasm-contracts"))]
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}

	#[test]
	fn guide_user_to_select_command_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_select(
			"What would you like to create?",
			Some(false),
			true,
			Some(vec![
				("Chain".into(), "Build your own custom chain".into()),
				("Pallet".into(), "Create reusable and customizable chain modules".into()),
				("Smart Contract".into(), "Write ink! smart contracts".into()),
			]),
			0,
			None,
		);
		assert_eq!(guide_user_to_select_command(&mut cli)?.to_string(), "chain");
		cli.verify()
	}
}
