// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use anyhow::Result as AnyhowResult;
use clap::{Args, Subcommand};
use serde::Serialize;
use std::fmt::{Display, Formatter, Result};

/// Structured output emitted by `new` subcommands in JSON mode.
#[derive(Debug, Serialize)]
pub(crate) struct NewOutput {
	pub kind: String,
	pub name: String,
	pub path: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub template: Option<String>,
}

/// Metadata for a single template entry in JSON listing output.
#[derive(Debug, Serialize)]
pub(crate) struct TemplateInfo {
	pub name: String,
	pub description: String,
}

/// Root `--list` output containing all template categories.
#[derive(Debug, Serialize)]
pub(crate) struct TemplateListOutput {
	#[cfg(feature = "chain")]
	pub chain_templates: Vec<TemplateInfo>,
	#[cfg(feature = "contract")]
	pub contract_templates: Vec<TemplateInfo>,
}

/// Subcommand-level `--list` output for a single template category.
#[derive(Debug, Serialize)]
pub(crate) struct SubcommandTemplateListOutput {
	pub templates: Vec<TemplateInfo>,
}

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
	fn new_output_serializes() {
		let output = NewOutput {
			kind: "contract".into(),
			name: "flipper".into(),
			path: "/tmp/flipper".into(),
			template: Some("Standard".into()),
		};
		let json = serde_json::to_value(&output).unwrap();
		assert_eq!(json["kind"], "contract");
		assert_eq!(json["name"], "flipper");
		assert_eq!(json["path"], "/tmp/flipper");
		assert_eq!(json["template"], "Standard");
	}

	#[test]
	fn new_output_without_template_skips_field() {
		let output = NewOutput {
			kind: "pallet".into(),
			name: "my-pallet".into(),
			path: "/tmp/my-pallet".into(),
			template: None,
		};
		let json = serde_json::to_value(&output).unwrap();
		assert!(json.get("template").is_none());
	}

	#[test]
	fn template_list_output_serializes() {
		let output = TemplateListOutput {
			#[cfg(feature = "chain")]
			chain_templates: vec![TemplateInfo {
				name: "standard".into(),
				description: "A standard chain".into(),
			}],
			#[cfg(feature = "contract")]
			contract_templates: vec![TemplateInfo {
				name: "erc20".into(),
				description: "An ERC20 token".into(),
			}],
		};
		let json = serde_json::to_value(&output).unwrap();
		#[cfg(feature = "chain")]
		{
			assert_eq!(json["chain_templates"][0]["name"], "standard");
			assert_eq!(json["chain_templates"][0]["description"], "A standard chain");
		}
		#[cfg(feature = "contract")]
		{
			assert_eq!(json["contract_templates"][0]["name"], "erc20");
			assert_eq!(json["contract_templates"][0]["description"], "An ERC20 token");
		}
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
