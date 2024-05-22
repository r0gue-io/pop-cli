// SPDX-License-Identifier: GPL-3.0
use strum::{
	EnumMessage as EnumMessageT, EnumProperty as EnumPropertyT, VariantArray as VariantArrayT,
};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};
use thiserror::Error;

/// Supported template providers.
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
	/// Get the list of providers supported.
	pub fn providers() -> &'static [Provider] {
		Provider::VARIANTS
	}

	/// Get provider's name.
	pub fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the default template of the provider.
	pub fn default_template(&self) -> Template {
		match &self {
			Provider::Pop => Template::Standard,
			Provider::Parity => Template::ParityContracts,
		}
	}

	/// Get the providers detailed description message.
	pub fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the list of templates of the provider.
	pub fn templates(&self) -> Vec<&Template> {
		Template::VARIANTS
			.iter()
			.filter(|t| t.get_str("Provider") == Some(self.name()))
			.collect()
	}
}

/// Configurable settings for parachain generation.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
	pub symbol: String,
	pub decimals: u8,
	pub initial_endowment: String,
}

/// Templates supported.
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
	/// Minimalist parachain template.
	#[default]
	#[strum(
		serialize = "standard",
		message = "Standard",
		detailed_message = "A standard parachain",
		props(Provider = "Pop", Repository = "https://github.com/r0gue-io/base-parachain")
	)]
	Standard,
	/// Parachain configured with fungible and non-fungilble asset functionalities.
	#[strum(
		serialize = "assets",
		message = "Assets",
		detailed_message = "Parachain configured with fungible and non-fungilble asset functionalities.",
		props(Provider = "Pop", Repository = "https://github.com/r0gue-io/assets-parachain")
	)]
	Assets,
	/// Parachain configured to support WebAssembly smart contracts.
	#[strum(
		serialize = "contracts",
		message = "Contracts",
		detailed_message = "Parachain configured to support WebAssembly smart contracts.",
		props(Provider = "Pop", Repository = "https://github.com/r0gue-io/contracts-parachain")
	)]
	Contracts,
	/// Parachain configured with Frontier, enabling compatibility with the Ethereum Virtual Machine (EVM).
	#[strum(
		serialize = "evm",
		message = "EVM",
		detailed_message = "Parachain configured with Frontier, enabling compatibility with the Ethereum Virtual Machine (EVM).",
		props(Provider = "Pop", Repository = "https://github.com/r0gue-io/evm-parachain")
	)]
	EVM,
	/// Minimal Substrate node configured for smart contracts via pallet-contracts.
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
	/// Template node for a Frontier (EVM) based parachain.
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
	/// Get the template's name.
	pub fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the detailed message of the template.
	pub fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Check the template belongs to a `provider`.
	pub fn matches(&self, provider: &Provider) -> bool {
		// Match explicitly on provider name (message)
		self.get_str("Provider") == Some(provider.name())
	}

	/// Get the template's repository url.
	pub fn repository_url(&self) -> Result<&str, Error> {
		self.get_str("Repository").ok_or(Error::RepositoryMissing)
	}

	/// Get the provider of the template.
	pub fn provider(&self) -> Result<&str, Error> {
		self.get_str("Provider").ok_or(Error::ProviderMissing)
	}
}

#[derive(Error, Debug)]
pub enum Error {
	#[error("The `Repository` property is missing from the template variant")]
	RepositoryMissing,
	#[error("The `Provider` property is missing from the template variant")]
	ProviderMissing,
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{collections::HashMap, str::FromStr};

	fn templates_names() -> HashMap<String, Template> {
		HashMap::from([
			("standard".to_string(), Template::Standard),
			("assets".to_string(), Template::Assets),
			("contracts".to_string(), Template::Contracts),
			("evm".to_string(), Template::EVM),
			("cpt".to_string(), Template::ParityContracts),
			("fpt".to_string(), Template::ParityFPT),
		])
	}
	fn templates_urls() -> HashMap<String, &'static str> {
		HashMap::from([
			("standard".to_string(), "https://github.com/r0gue-io/base-parachain"),
			("assets".to_string(), "https://github.com/r0gue-io/assets-parachain"),
			("contracts".to_string(), "https://github.com/r0gue-io/contracts-parachain"),
			("evm".to_string(), "https://github.com/r0gue-io/evm-parachain"),
			("cpt".to_string(), "https://github.com/paritytech/substrate-contracts-node"),
			("fpt".to_string(), "https://github.com/paritytech/frontier-parachain-template"),
		])
	}

	#[test]
	fn test_is_template_correct() {
		for template in Template::VARIANTS {
			if matches!(
				template,
				Template::Standard | Template::Assets | Template::Contracts | Template::EVM
			) {
				assert_eq!(template.matches(&Provider::Pop), true);
				assert_eq!(template.matches(&Provider::Parity), false);
			}
			if matches!(template, Template::ParityContracts | Template::ParityFPT) {
				assert_eq!(template.matches(&Provider::Pop), false);
				assert_eq!(template.matches(&Provider::Parity), true);
			}
		}
	}

	#[test]
	fn test_convert_string_to_template() {
		let template_names = templates_names();
		// Test the default
		assert_eq!(Template::from_str("").unwrap_or_default(), Template::Standard);
		// Test the rest
		for template in Template::VARIANTS {
			assert_eq!(
				&Template::from_str(&template.to_string()).unwrap(),
				template_names.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_repository_url() {
		let template_urls = templates_urls();
		for template in Template::VARIANTS {
			assert_eq!(
				&template.repository_url().unwrap(),
				template_urls.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_default_template_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.default_template(), Template::Standard);
		provider = Provider::Parity;
		assert_eq!(provider.default_template(), Template::ParityContracts);
	}

	#[test]
	fn test_templates_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(
			provider.templates(),
			[&Template::Standard, &Template::Assets, &Template::Contracts, &Template::EVM]
		);
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
