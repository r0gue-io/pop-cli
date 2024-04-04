use clap::Parser;
use strum_macros::{Display, EnumString};

use cliclack;

#[derive(Debug, Clone)]
pub struct Config {
	pub(crate) symbol: String,
	pub(crate) decimals: u8,
	pub(crate) initial_endowment: String,
}


#[derive(Clone, Parser, Debug, Display, EnumString, PartialEq)]
pub enum Template {
	// Pop
	#[strum(serialize = "Standard Template", serialize = "base")]
	Base,
	// OpenZeppelin
	#[strum(serialize = "Generic Template", serialize = "template")]
	OZTemplate,
	// Parity
	#[strum(serialize = "Parity Contracts Node Template", serialize = "cpt")]
	ParityContracts,
	#[strum(serialize = "Parity Frontier Parachain Template", serialize = "fpt")]
	ParityFPT,
}
impl Template {
	pub fn is_provider_correct(&self, provider: &Provider) -> bool {
		match provider {
			Provider::Pop => self == &Template::Base,
			Provider::OpenZeppelin => self == &Template::OZTemplate,
			Provider::Parity => self == &Template::ParityContracts || self == &Template::ParityFPT,
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
	#[strum(ascii_case_insensitive)]
	Pop,
	#[strum(ascii_case_insensitive)]
	OpenZeppelin,
	#[strum(ascii_case_insensitive)]
	Parity,
}
impl Provider {
	pub fn default_template(&self) -> Template {
		match &self {
			Provider::Pop => Template::Base,
			Provider::OpenZeppelin => Template::OZTemplate,
			Provider::Parity => Template::ParityContracts,
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
			Provider::Pop => cliclack::select("Select the type of parachain:".to_string())
				.initial_value("base")
				.item("base", "Standard Template", "A standard parachain")
				.interact()
				.expect("Error parsing user input"),
			Provider::OpenZeppelin => cliclack::select("Select the type of parachain:".to_string())
				.initial_value("template")
				.item("template", "Generic Template", "A generic template for Substrate Runtime.")
				.interact()
				.expect("Error parsing user input"),
			Provider::Parity => cliclack::select("Select the type of parachain:".to_string())
				.initial_value("cpt")
				.item(
					"cpt",
					"Contracts",
					"Minimal Substrate node configured for smart contracts via pallet-contracts.",
				)
				.item("fpt", "EVM", "Template node for a Frontier (EVM) based parachain.")
				.interact()
				.expect("Error parsing user input"),
		}
	}
}
