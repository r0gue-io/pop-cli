use clap::Parser;
use strum_macros::{Display, EnumString};

use cliclack;

#[derive(Clone, Parser, Debug, Display, EnumString, PartialEq)]
pub enum Template {
	#[strum(serialize = "Pop Base Parachain Template", serialize = "base")]
	Base,
	#[strum(serialize = "OpenZeppeling Runtime Parachain Template", serialize = "template")]
	OZTemplate,
	#[strum(serialize = "Parity Contracts Node Template", serialize = "cpt")]
	ParityContracts,
	#[strum(serialize = "Parity Frontier Parachain Template", serialize = "fpt")]
	ParityFPT,
}
impl Template {
	pub fn is_provider_correct(&self, provider: &Provider) -> bool {
		match provider {
			Provider::Pop => return self == &Template::Base,
			Provider::OpenZeppelin => return self == &Template::OZTemplate,
			Provider::Parity => {
				return self == &Template::ParityContracts || self == &Template::ParityFPT
			},
		}
	}
	pub fn from(provider_name: &str) -> Self {
		match provider_name {
			"base" => Template::Base,
			"template" => Template::OZTemplate,
			"cpt" => Template::ParityContracts,
			"fpt" => Template::ParityFPT,
			_ => Template::Base,
		}
	}
	pub fn repository_url(&self) -> &str {
		match &self {
			Template::Base => "https://github.com/r0gue-io/base-parachain",
			Template::OZTemplate => "https://github.com/OpenZeppelin/polkadot-runtime-template",
			Template::ParityContracts => "https://github.com/paritytech/substrate-contracts-node",
			Template::ParityFPT => "https://github.com/paritytech/frontier-parachain-template",
		}
	}
}

#[derive(Clone, Default, Parser, Debug, Display, EnumString, PartialEq)]
pub enum Provider {
	#[default]
	#[strum(serialize = "Pop", serialize = "pop")]
	Pop,
	#[strum(serialize = "OpenZeppelin", serialize = "openzeppelin")]
	OpenZeppelin,
	#[strum(serialize = "Parity", serialize = "parity")]
	Parity,
}
impl Provider {
	pub fn default_template(&self) -> Template {
		match &self {
			Provider::Pop => return Template::Base,
			Provider::OpenZeppelin => return Template::OZTemplate,
			Provider::Parity => return Template::ParityContracts,
		}
	}
	pub fn from(provider_name: &str) -> Self {
		match provider_name {
			"Pop" => Provider::Pop,
			"OpenZeppelin" => Provider::OpenZeppelin,
			"Parity" => Provider::Parity,
			_ => Provider::Pop,
		}
	}
	pub fn display_select_options(&self) -> &str {
		match &self {
			Provider::Pop => {
				return cliclack::select(format!("Select the type of parachain:"))
					.initial_value("base")
					.item("base", "Base Parachain", "A standard parachain")
					.interact()
					.expect("Error parsing user input");
			},
			Provider::OpenZeppelin => {
				return cliclack::select(format!("Select the type of parachain:"))
					.initial_value("template")
					.item(
						"template",
						"OpenZeppeling Template",
						"OpenZeppeling Runtime Parachain Template",
					)
					.interact()
					.expect("Error parsing user input");
			},
			Provider::Parity => {
				return cliclack::select(format!("Select the type of parachain:"))
					.initial_value("cpt")
					.item("cpt", "Parity Contracts", "A parachain supporting WebAssembly smart contracts such as ink!.")
					.item("fpt", "Parity EVM", "A parachain supporting smart contracts targeting the Ethereum Virtual Machine (EVM).")
					.interact()
					.expect("Error parsing user input");
			},
		};
	}
}
