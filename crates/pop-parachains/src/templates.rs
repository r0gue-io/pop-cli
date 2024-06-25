// SPDX-License-Identifier: GPL-3.0

use pop_common::templates::{Template, TemplateType};
use strum::EnumProperty as _;
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
		serialize = "openzeppelin",
		message = "OpenZeppelin",
		detailed_message = "The standard for secure blockchain applications."
	)]
	OpenZeppelin,
	#[strum(
		ascii_case_insensitive,
		serialize = "parity",
		message = "Parity",
		detailed_message = "Solutions for a trust-free world."
	)]
	Parity,
}

impl TemplateType<ParachainTemplate> for Provider {
	const TYPE_ID: &'static str = "Provider";

	fn default_template(&self) -> ParachainTemplate {
		match &self {
			Provider::Pop => ParachainTemplate::Standard,
			Provider::OpenZeppelin => ParachainTemplate::OpenZeppelinGeneric,
			Provider::Parity => ParachainTemplate::ParityContracts,
		}
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
	Hash,
	PartialEq,
	VariantArray,
)]
pub enum ParachainTemplate {
	/// Minimalist parachain template.
	#[default]
	#[strum(
		serialize = "standard",
		message = "Standard",
		detailed_message = "A standard parachain",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/base-parachain",
			Network = "./network.toml"
		)
	)]
	Standard,
	/// Parachain configured with fungible and non-fungilble asset functionalities.
	#[strum(
		serialize = "assets",
		message = "Assets",
		detailed_message = "Parachain configured with fungible and non-fungilble asset functionalities.",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/assets-parachain",
			Network = "./network.toml"
		)
	)]
	Assets,
	/// Parachain configured to support WebAssembly smart contracts.
	#[strum(
		serialize = "contracts",
		message = "Contracts",
		detailed_message = "Parachain configured to support WebAssembly smart contracts.",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/contracts-parachain",
			Network = "./network.toml"
		)
	)]
	Contracts,
	/// Parachain configured with Frontier, enabling compatibility with the Ethereum Virtual Machine (EVM).
	#[strum(
		serialize = "evm",
		message = "EVM",
		detailed_message = "Parachain configured with Frontier, enabling compatibility with the Ethereum Virtual Machine (EVM).",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/evm-parachain",
			Network = "./network.toml"
		)
	)]
	EVM,
	// OpenZeppelin
	#[strum(
		serialize = "polkadot-generic-runtime-template",
		message = "Generic Runtime Template",
		detailed_message = "A generic template for Substrate Runtime",
		props(
			Provider = "OpenZeppelin",
			Repository = "https://github.com/OpenZeppelin/polkadot-runtime-templates",
			Network = "./zombienet-config/devnet.toml",
			SupportedVersions = "v1.0.0",
			IsAudited = "true"
		)
	)]
	OpenZeppelinGeneric,
	/// Minimal Substrate node configured for smart contracts via pallet-contracts.
	#[strum(
		serialize = "cpt",
		message = "Contracts",
		detailed_message = "Minimal Substrate node configured for smart contracts via pallet-contracts.",
		props(
			Provider = "Parity",
			Repository = "https://github.com/paritytech/substrate-contracts-node",
			Network = "./zombienet.toml"
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
			Repository = "https://github.com/paritytech/frontier-parachain-template",
			Network = "./zombienet-config.toml"
		)
	)]
	ParityFPT,

	// templates for unit tests below
	#[cfg(test)]
	#[strum(
		serialize = "test_01",
		message = "Test_01",
		detailed_message = "Test template only compiled in test mode.",
		props(
			Provider = "Test",
			Repository = "",
			Network = "",
			SupportedVersions = "v1.0.0,v2.0.0",
			IsAudited = "true"
		)
	)]
	TestTemplate01,
	#[cfg(test)]
	#[strum(
		serialize = "test_02",
		message = "Test_02",
		detailed_message = "Test template only compiled in test mode.",
		props(Provider = "Test", Repository = "", Network = "",)
	)]
	TestTemplate02,
}

impl Template for ParachainTemplate {}

impl ParachainTemplate {
	/// Get the provider of the template.
	pub fn provider(&self) -> Result<&str, Error> {
		self.get_str("Provider").ok_or(Error::ProviderMissing)
	}

	/// Returns the relative path to the default network configuration file to be used, if defined.
	pub fn network_config(&self) -> Option<&str> {
		self.get_str("Network")
	}

	pub fn supported_versions(&self) -> Option<Vec<&str>> {
		self.get_str("SupportedVersions").map(|s| s.split(',').collect())
	}

	pub fn is_supported_version(&self, version: &str) -> bool {
		// if `SupportedVersion` is None, then all versions are supported. Otherwise, ensure version is present.
		self.supported_versions().map_or(true, |versions| versions.contains(&version))
	}

	pub fn is_audited(&self) -> bool {
		self.get_str("IsAudited").map_or(false, |s| s == "true")
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
	use strum::VariantArray;

	fn templates_names() -> HashMap<String, ParachainTemplate> {
		HashMap::from([
			("standard".to_string(), ParachainTemplate::Standard),
			("assets".to_string(), ParachainTemplate::Assets),
			("contracts".to_string(), ParachainTemplate::Contracts),
			("evm".to_string(), ParachainTemplate::EVM),
			// openzeppelin
			(
				"polkadot-generic-runtime-template".to_string(),
				ParachainTemplate::OpenZeppelinGeneric,
			),
			("cpt".to_string(), ParachainTemplate::ParityContracts),
			("fpt".to_string(), ParachainTemplate::ParityFPT),
			("test_01".to_string(), ParachainTemplate::TestTemplate01),
			("test_02".to_string(), ParachainTemplate::TestTemplate02),
		])
	}

	fn templates_urls() -> HashMap<String, &'static str> {
		HashMap::from([
			("standard".to_string(), "https://github.com/r0gue-io/base-parachain"),
			("assets".to_string(), "https://github.com/r0gue-io/assets-parachain"),
			("contracts".to_string(), "https://github.com/r0gue-io/contracts-parachain"),
			("evm".to_string(), "https://github.com/r0gue-io/evm-parachain"),
			// openzeppelin
			(
				"polkadot-generic-runtime-template".to_string(),
				"https://github.com/OpenZeppelin/polkadot-runtime-templates",
			),
			("cpt".to_string(), "https://github.com/paritytech/substrate-contracts-node"),
			("fpt".to_string(), "https://github.com/paritytech/frontier-parachain-template"),
			("test_01".to_string(), ""),
			("test_02".to_string(), ""),
		])
	}

	fn template_network_configs() -> HashMap<ParachainTemplate, Option<&'static str>> {
		[
			(ParachainTemplate::Standard, Some("./network.toml")),
			(ParachainTemplate::Assets, Some("./network.toml")),
			(ParachainTemplate::Contracts, Some("./network.toml")),
			(ParachainTemplate::EVM, Some("./network.toml")),
			(ParachainTemplate::OpenZeppelinGeneric, Some("./zombienet-config/devnet.toml")),
			(ParachainTemplate::ParityContracts, Some("./zombienet.toml")),
			(ParachainTemplate::ParityFPT, Some("./zombienet-config.toml")),
			(ParachainTemplate::TestTemplate01, Some("")),
			(ParachainTemplate::TestTemplate02, Some("")),
		]
		.into()
	}

	#[test]
	fn test_is_template_correct() {
		for template in ParachainTemplate::VARIANTS {
			if matches!(
				template,
				ParachainTemplate::Standard
					| ParachainTemplate::Assets
					| ParachainTemplate::Contracts
					| ParachainTemplate::EVM
			) {
				assert_eq!(Provider::Pop.matches(&template), true);
				assert_eq!(Provider::Parity.matches(&template), false);
			}
			if matches!(template, ParachainTemplate::ParityContracts | ParachainTemplate::ParityFPT)
			{
				assert_eq!(Provider::Pop.matches(&template), false);
				assert_eq!(Provider::Parity.matches(&template), true)
			}
		}
	}

	#[test]
	fn test_convert_string_to_template() {
		let template_names = templates_names();
		// Test the default
		assert_eq!(
			ParachainTemplate::from_str("").unwrap_or_default(),
			ParachainTemplate::Standard
		);
		// Test the rest
		for template in ParachainTemplate::VARIANTS {
			assert_eq!(
				&ParachainTemplate::from_str(&template.to_string()).unwrap(),
				template_names.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_repository_url() {
		let template_urls = templates_urls();
		for template in ParachainTemplate::VARIANTS {
			assert_eq!(
				&template.repository_url().unwrap(),
				template_urls.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_network_config() {
		let network_configs = template_network_configs();
		for template in ParachainTemplate::VARIANTS {
			assert_eq!(template.network_config(), network_configs[template]);
		}
	}

	#[test]
	fn test_default_template_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.default_template(), ParachainTemplate::Standard);
		provider = Provider::Parity;
		assert_eq!(provider.default_template(), ParachainTemplate::ParityContracts);
	}

	#[test]
	fn test_templates_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(
			provider.templates(),
			[
				&ParachainTemplate::Standard,
				&ParachainTemplate::Assets,
				&ParachainTemplate::Contracts,
				&ParachainTemplate::EVM
			]
		);
		provider = Provider::Parity;
		assert_eq!(
			provider.templates(),
			[&ParachainTemplate::ParityContracts, &ParachainTemplate::ParityFPT]
		);
	}

	#[test]
	fn test_convert_string_to_provider() {
		assert_eq!(Provider::from_str("Pop").unwrap(), Provider::Pop);
		assert_eq!(Provider::from_str("").unwrap_or_default(), Provider::Pop);
		assert_eq!(Provider::from_str("Parity").unwrap(), Provider::Parity);
	}

	#[test]
	fn supported_versions_have_no_whitespace() {
		for template in ParachainTemplate::VARIANTS {
			if let Some(versions) = template.supported_versions() {
				for version in versions {
					assert!(!version.contains(' '));
				}
			}
		}
	}

	#[test]
	fn test_supported_versions_works() {
		let template = ParachainTemplate::TestTemplate01;
		assert_eq!(template.supported_versions(), Some(vec!["v1.0.0", "v2.0.0"]));
		assert_eq!(template.is_supported_version("v1.0.0"), true);
		assert_eq!(template.is_supported_version("v2.0.0"), true);
		assert_eq!(template.is_supported_version("v3.0.0"), false);

		let template = ParachainTemplate::TestTemplate02;
		assert_eq!(template.supported_versions(), None);
		// will be true because an empty SupportedVersions defaults to all
		assert_eq!(template.is_supported_version("v1.0.0"), true);
	}

	#[test]
	fn test_is_audited() {
		let template = ParachainTemplate::TestTemplate01;
		assert_eq!(template.is_audited(), true);

		let template = ParachainTemplate::TestTemplate02;
		assert_eq!(template.is_audited(), false);
	}
}
