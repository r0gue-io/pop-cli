// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use anyhow::Result as AnyhowResult;
use clap::{Args, Subcommand};
use serde::Serialize;
use std::fmt::{Display, Formatter, Result};

#[cfg(feature = "chain")]
pub mod chain;
#[cfg(feature = "contract")]
pub mod contract;
/// Utilities for selecting a frontend template and generate it.
pub mod frontend;
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
#[derive(Args, Serialize)]
#[command(args_conflicts_with_subcommands = true)]
pub struct NewArgs {
	#[command(subcommand)]
	pub command: Option<Command>,
	/// List available templates.
	#[arg(short, long)]
	pub list: bool,
}

/// Generate a new parachain, pallet or smart contract.
#[derive(Subcommand, Serialize)]
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
	#[cfg(feature = "contract")]
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
			#[cfg(feature = "contract")]
			Command::Contract(_) => write!(f, "contract"),
		}
	}
}

/// Guide the user to select what type of project to create
pub fn guide_user_to_select_command(cli: &mut impl Cli) -> AnyhowResult<Command> {
	cli.intro("Welcome to Pop CLI!")?;

	let mut prompt = cli.select("What would you like to create?".to_string());

	// Add available options based on features
	#[cfg(feature = "chain")]
	{
		prompt = prompt.item("chain", "Chain", "Build your own custom chain");
		prompt = prompt.item("pallet", "Pallet", "Create reusable and customizable chain modules");
	}

	#[cfg(feature = "contract")]
	{
		prompt = prompt.item("contract", "Smart Contract", "Write ink! smart contracts");
	}

	// Set initial selection to the first available option
	#[cfg(feature = "chain")]
	{
		prompt = prompt.initial_value("chain");
	}
	#[cfg(all(feature = "contract", not(feature = "chain")))]
	{
		prompt = prompt.initial_value("contract");
	}

	let selection = prompt.interact()?;

	match selection {
		#[cfg(feature = "chain")]
		"chain" => Ok(Command::Chain(chain::NewChainCommand {
			name: None,
			template: None,
			release_tag: None,
			symbol: None,
			decimals: None,
			initial_endowment: None,
			verify: false,
			list: false,
			with_frontend: None,
			package_manager: None,
		})),
		#[cfg(feature = "chain")]
		"pallet" => Ok(Command::Pallet(pallet::NewPalletCommand {
			name: None,
			authors: None,
			description: None,
			mode: None,
		})),
		#[cfg(feature = "contract")]
		"contract" => Ok(Command::Contract(contract::NewContractCommand {
			name: None,
			template: None,
			list: false,
			with_frontend: None,
			package_manager: None,
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
		#[cfg(feature = "contract")]
		assert_eq!(Command::Contract(Default::default()).to_string(), "contract");
	}

	#[test]
	fn guide_user_to_select_command_works() -> anyhow::Result<()> {
		let mut options = Vec::new();
		#[cfg(feature = "chain")]
		{
			options.push(("Chain".into(), "Build your own custom chain".into()));
			options
				.push(("Pallet".into(), "Create reusable and customizable chain modules".into()));
		}

		#[cfg(feature = "contract")]
		{
			options.push(("Smart Contract".into(), "Write ink! smart contracts".into()));
		}
		let mut cli = MockCli::new().expect_select(
			"What would you like to create?",
			Some(false),
			true,
			Some(options),
			0,
			None,
		);
		let cmd = guide_user_to_select_command(&mut cli)?;
		#[cfg(feature = "chain")]
		assert_eq!(cmd.to_string(), "chain");

		#[cfg(all(not(feature = "chain"), feature = "contract"))]
		assert_eq!(cmd.to_string(), "contract");
		cli.verify()
	}
}
