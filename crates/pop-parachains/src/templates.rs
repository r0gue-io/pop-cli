// SPDX-License-Identifier: GPL-3.0
use strum::{
	EnumMessage as EnumMessageT, EnumProperty as EnumPropertyT, VariantArray as VariantArrayT,
};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};
use thiserror::Error;

#[derive(
	AsRefStr, Clone, Default, Debug, Display, EnumMessage, EnumString, Eq, PartialEq, VariantArray,
)]
pub enum Provider {
	#[default]
	#[strum(
		ascii_case_insensitive,
		serialize = "pop",
		message = "Pop",
		detailed_message = "An all-in-one tool for Polkadot development."
	)]
	Pop,
	#[strum(
		ascii_case_insensitive,
		serialize = "parity",
		message = "Parity",
		detailed_message = "Solutions for a trust-free world."
	)]
	Parity,
}

impl Provider {
	pub fn providers() -> &'static [Provider] {
		Provider::VARIANTS
	}

	pub fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	pub fn default_template(&self) -> Template {
		match &self {
			Provider::Pop => Template::Base,
			Provider::Parity => Template::ParityContracts,
		}
	}

	pub fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	pub fn templates(&self) -> Vec<&Template> {
		Template::VARIANTS
			.iter()
			.filter(|t| t.get_str("Provider") == Some(self.name()))
			.collect()
	}
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
	pub symbol: String,
	pub decimals: u8,
	pub initial_endowment: String,
}

#[derive(
	AsRefStr,
	Clone,
	Debug,
	Default,
	Display,
	EnumMessage,
	EnumProperty,
	EnumString,
	Eq,
	PartialEq,
	VariantArray,
)]
pub enum Template {
	// Pop
	#[default]
	#[strum(
		serialize = "base",
		message = "Standard",
		detailed_message = "A standard parachain",
		props(Provider = "Pop", Repository = "https://github.com/r0gue-io/base-parachain")
	)]
	Base,
	#[strum(
		serialize = "assets",
		message = "Assets",
		detailed_message = "Parachain configured with fungible and non-fungilble asset functionalities.",
		props(Provider = "Pop", Repository = "https://github.com/r0gue-io/assets-parachain")
	)]
	Assets,
	// Parity
	#[strum(
		serialize = "cpt",
		message = "Contracts",
		detailed_message = "Minimal Substrate node configured for smart contracts via pallet-contracts.",
		props(
			Provider = "Parity",
			Repository = "https://github.com/paritytech/substrate-contracts-node"
		)
	)]
	ParityContracts,
	#[strum(
		serialize = "fpt",
		message = "EVM",
		detailed_message = "Template node for a Frontier (EVM) based parachain.",
		props(
			Provider = "Parity",
			Repository = "https://github.com/paritytech/frontier-parachain-template"
		)
	)]
	ParityFPT,
}

impl Template {
	pub fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}
	pub fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	pub fn matches(&self, provider: &Provider) -> bool {
		// Match explicitly on provider name (message)
		self.get_str("Provider") == Some(provider.name())
	}

	pub fn repository_url(&self) -> Result<&str, Error> {
		self.get_str("Repository").ok_or(Error::RepositoryMissing)
	}
}

#[derive(Error, Debug)]
pub enum Error {
	#[error("The `Repository` property is missing from the template variant")]
	RepositoryMissing,
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::str::FromStr;

	#[test]
	fn test_is_template_correct() {
		let mut template = Template::Base;
		assert_eq!(template.matches(&Provider::Pop), true);
		assert_eq!(template.matches(&Provider::Parity), false);

		template = Template::ParityContracts;
		assert_eq!(template.matches(&Provider::Pop), false);
		assert_eq!(template.matches(&Provider::Parity), true);

		template = Template::ParityFPT;
		assert_eq!(template.matches(&Provider::Pop), false);
		assert_eq!(template.matches(&Provider::Parity), true);

		template = Template::Assets;
		assert_eq!(template.matches(&Provider::Pop), true);
		assert_eq!(template.matches(&Provider::Parity), false);
	}

	#[test]
	fn test_convert_string_to_template() {
		assert_eq!(Template::from_str("base").unwrap(), Template::Base);
		assert_eq!(Template::from_str("").unwrap_or_default(), Template::Base);
		assert_eq!(Template::from_str("assets").unwrap(), Template::Assets);
		assert_eq!(Template::from_str("cpt").unwrap(), Template::ParityContracts);
		assert_eq!(Template::from_str("fpt").unwrap(), Template::ParityFPT);
	}

	#[test]
	fn test_repository_url() {
		let mut template = Template::Base;
		assert_eq!(
			template.repository_url().unwrap(),
			"https://github.com/r0gue-io/base-parachain"
		);
		template = Template::ParityContracts;
		assert_eq!(
			template.repository_url().unwrap(),
			"https://github.com/paritytech/substrate-contracts-node"
		);
		template = Template::ParityFPT;
		assert_eq!(
			template.repository_url().unwrap(),
			"https://github.com/paritytech/frontier-parachain-template"
		);
		template = Template::Assets;
		assert_eq!(
			template.repository_url().unwrap(),
			"https://github.com/r0gue-io/assets-parachain"
		);
	}

	#[test]
	fn test_default_template_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.default_template(), Template::Base);
		provider = Provider::Parity;
		assert_eq!(provider.default_template(), Template::ParityContracts);
	}

	#[test]
	fn test_templates_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.templates(), [&Template::Base, &Template::Assets]);
		provider = Provider::Parity;
		assert_eq!(provider.templates(), [&Template::ParityContracts, &Template::ParityFPT]);
	}

	#[test]
	fn test_convert_string_to_provider() {
		assert_eq!(Provider::from_str("Pop").unwrap(), Provider::Pop);
		assert_eq!(Provider::from_str("").unwrap_or_default(), Provider::Pop);
		assert_eq!(Provider::from_str("Parity").unwrap(), Provider::Parity);
	}
}
