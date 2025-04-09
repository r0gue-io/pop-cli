// SPDX-License-Identifier: GPL-3.0

use pop_common::templates::{Template, Type};
use strum::{EnumProperty as _, VariantArray};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString};

/// Supported template providers.
#[derive(
	AsRefStr, Clone, Default, Debug, Display, EnumMessage, EnumString, Eq, PartialEq, VariantArray,
)]
pub enum Provider {
	/// Pop: An all-in-one tool for Polkadot development.
	#[default]
	#[strum(
		ascii_case_insensitive,
		serialize = "pop",
		message = "Pop",
		detailed_message = "An all-in-one tool for Polkadot development."
	)]
	Pop,
	/// OpenZeppelin: The standard for secure blockchain applications.
	#[strum(
		ascii_case_insensitive,
		serialize = "openzeppelin",
		message = "OpenZeppelin",
		detailed_message = "The standard for secure blockchain applications."
	)]
	OpenZeppelin,
	/// Parity: Solutions for a trust-free world.
	#[strum(
		ascii_case_insensitive,
		serialize = "parity",
		message = "Parity",
		detailed_message = "Solutions for a trust-free world."
	)]
	Parity,
}

impl Type<Parachain> for Provider {
	fn default_template(&self) -> Option<Parachain> {
		match &self {
			Provider::Pop => Some(Parachain::Standard),
			Provider::OpenZeppelin => Some(Parachain::OpenZeppelinGeneric),
			Provider::Parity => Some(Parachain::ParityGeneric),
		}
	}
}

/// Configurable settings for parachain generation.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
	/// The token symbol.
	pub symbol: String,
	/// The number of decimals used for the token.
	pub decimals: u8,
	/// The initial endowment amount.
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
pub enum Parachain {
	/// Pop Standard Template: Minimalist parachain template.
	#[default]
	#[strum(
		serialize = "r0gue-io/base-parachain",
		message = "Standard",
		detailed_message = "A standard parachain",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/base-parachain",
			Network = "./network.toml",
			License = "Unlicense",
			DeploymentName = "POP_STANDARD"
		)
	)]
	Standard,
	/// Pop Assets Template: Parachain configured with fungible and non-fungible asset
	/// functionalities.
	#[strum(
		serialize = "r0gue-io/assets-parachain",
		message = "Assets",
		detailed_message = "Parachain configured with fungible and non-fungible asset functionalities.",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/assets-parachain",
			Network = "./network.toml",
			License = "Unlicense",
			DeploymentName = "POP_ASSETS"
		)
	)]
	Assets,
	/// Pop Contracts Template: Parachain configured to support WebAssembly smart contracts.
	#[strum(
		serialize = "r0gue-io/contracts-parachain",
		message = "Contracts",
		detailed_message = "Parachain configured to support WebAssembly smart contracts.",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/contracts-parachain",
			Network = "./network.toml",
			License = "Unlicense",
			DeploymentName = "POP_CONTRACTS"
		)
	)]
	Contracts,
	/// Pop EVM Template: Parachain configured with Frontier, enabling compatibility with the
	/// Ethereum Virtual Machine (EVM).
	#[strum(
		serialize = "r0gue-io/evm-parachain",
		message = "EVM",
		detailed_message = "Parachain configured with Frontier, enabling compatibility with the Ethereum Virtual Machine (EVM).",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/evm-parachain",
			Network = "./network.toml",
			License = "Unlicense",
			DeploymentName = "POP_EVM"
		)
	)]
	EVM,
	/// OpenZeppelin Generic Runtime Template: A generic template for Substrate Runtime.
	#[strum(
		serialize = "openzeppelin/generic-template",
		message = "Generic Runtime Template",
		detailed_message = "A generic template for Substrate Runtime.",
		props(
			Provider = "OpenZeppelin",
			Repository = "https://github.com/OpenZeppelin/polkadot-runtime-templates",
			Network = "./zombienet-config/devnet.toml",
			SupportedVersions = "v1.0.0,v2.0.1,v2.0.3,v3.0.0",
			IsAudited = "true",
			License = "GPL-3.0",
			DeploymentName = "OZ_GENERIC"
		)
	)]
	OpenZeppelinGeneric,
	/// OpenZeppelin EVM Template: Parachain with EVM compatibility out of the box.
	#[strum(
		serialize = "openzeppelin/evm-template",
		message = "EVM Template",
		detailed_message = "Parachain with EVM compatibility out of the box.",
		props(
			Provider = "OpenZeppelin",
			Repository = "https://github.com/OpenZeppelin/polkadot-runtime-templates",
			Network = "./zombienet-config/devnet.toml",
			SupportedVersions = "v2.0.3,v3.0.0",
			IsAudited = "true",
			License = "GPL-3.0",
			DeploymentName = "OZ_EVM"
		)
	)]
	OpenZeppelinEVM,
	/// Parity Generic Template: The Parachain-Ready Template From Polkadot SDK.
	#[strum(
		serialize = "paritytech/polkadot-sdk-parachain-template",
		message = "Polkadot SDK's Parachain Template",
		detailed_message = "The Parachain-Ready Template From Polkadot SDK.",
		props(
			Provider = "Parity",
			Repository = "https://github.com/paritytech/polkadot-sdk-parachain-template",
			Network = "./zombienet.toml",
			License = "Unlicense",
			DeploymentName = "PARITY_GENERIC"
		)
	)]
	ParityGeneric,
	/// Parity Contracts Template: Minimal Substrate node configured for smart contracts via
	/// pallet-contracts and pallet-revive.
	#[strum(
		serialize = "paritytech/substrate-contracts-node",
		message = "Contracts",
		detailed_message = "Minimal Substrate node configured for smart contracts via pallet-contracts and pallet-revive.",
		props(
			Provider = "Parity",
			Repository = "https://github.com/paritytech/substrate-contracts-node",
			Network = "./zombienet.toml",
			License = "Unlicense"
		)
	)]
	ParityContracts,
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
			IsAudited = "true",
			IsDeprecated = "true",
			DeprecatedMessage = "This template is deprecated. Please use test_02 in the future.",
			License = "Unlicense",
		)
	)]
	TestTemplate01,
	#[cfg(test)]
	#[strum(
		serialize = "test_02",
		message = "Test_02",
		detailed_message = "Test template only compiled in test mode.",
		props(Provider = "Test", Repository = "", Network = "", License = "GPL-3.0")
	)]
	TestTemplate02,
}

impl Template for Parachain {
	const PROPERTY: &'static str = "Provider";
}

impl Parachain {
	/// Returns the relative path to the default network configuration file to be used, if defined.
	pub fn network_config(&self) -> Option<&str> {
		self.get_str("Network")
	}

	/// The supported versions of the template.
	pub fn supported_versions(&self) -> Option<Vec<&str>> {
		self.get_str("SupportedVersions").map(|s| s.split(',').collect())
	}

	/// Whether the specified version is supported.
	///
	/// # Arguments
	/// * `version`: The version to be checked.
	pub fn is_supported_version(&self, version: &str) -> bool {
		// if `SupportedVersion` is None, then all versions are supported. Otherwise, ensure version
		// is present.
		self.supported_versions().is_none_or(|versions| versions.contains(&version))
	}

	/// Whether the template has been audited.
	pub fn is_audited(&self) -> bool {
		self.get_str("IsAudited") == Some("true")
	}

	/// The license used.
	pub fn license(&self) -> Option<&str> {
		self.get_str("License")
	}

	/// Returns the deployment name for the parachain if defined.
	pub fn deployment_name(&self) -> Option<&str> {
		self.get_str("DeploymentName")
	}

	/// Retrieves the deployment name from the `based_on` value.
	pub fn deployment_name_from_based_on(based_on: &str) -> Option<String> {
		// OpenZeppelin special cases first (https://github.com/OpenZeppelin/polkadot-runtime-templates/pull/406)
		let mapped_based_on = match based_on {
			"OpenZeppelin EVM Template" => Some(Parachain::OpenZeppelinEVM),
			"OpenZeppelin Generic Template" => Some(Parachain::OpenZeppelinGeneric),
			_ => None,
		};
		if let Some(variant) = mapped_based_on {
			return variant.deployment_name().map(String::from);
		}
		Parachain::VARIANTS
			.iter()
			.find(|variant| variant.as_ref() == based_on)
			.and_then(|variant| variant.deployment_name().map(String::from))
	}

	/// Gets the template name, removing the provider if present.
	pub fn template_name_without_provider(&self) -> &str {
		let name = self.as_ref();
		name.split_once('/').map_or(name, |(_, template)| template)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{collections::HashMap, str::FromStr};
	use Parachain::*;

	fn templates_names() -> HashMap<String, Parachain> {
		HashMap::from([
			("r0gue-io/base-parachain".to_string(), Standard),
			("r0gue-io/assets-parachain".to_string(), Assets),
			("r0gue-io/contracts-parachain".to_string(), Contracts),
			("r0gue-io/evm-parachain".to_string(), EVM),
			// openzeppelin
			("openzeppelin/generic-template".to_string(), OpenZeppelinGeneric),
			("openzeppelin/evm-template".to_string(), OpenZeppelinEVM),
			// pàrity
			("paritytech/polkadot-sdk-parachain-template".to_string(), ParityGeneric),
			("paritytech/substrate-contracts-node".to_string(), ParityContracts),
			("test_01".to_string(), TestTemplate01),
			("test_02".to_string(), TestTemplate02),
		])
	}

	fn templates_names_without_providers() -> HashMap<Parachain, String> {
		HashMap::from([
			(Standard, "base-parachain".to_string()),
			(Assets, "assets-parachain".to_string()),
			(Contracts, "contracts-parachain".to_string()),
			(EVM, "evm-parachain".to_string()),
			(OpenZeppelinGeneric, "generic-template".to_string()),
			(OpenZeppelinEVM, "evm-template".to_string()),
			(ParityGeneric, "polkadot-sdk-parachain-template".to_string()),
			(ParityContracts, "substrate-contracts-node".to_string()),
			(TestTemplate01, "test_01".to_string()),
			(TestTemplate02, "test_02".to_string()),
		])
	}

	fn templates_urls() -> HashMap<String, &'static str> {
		HashMap::from([
			("r0gue-io/base-parachain".to_string(), "https://github.com/r0gue-io/base-parachain"),
			(
				"r0gue-io/assets-parachain".to_string(),
				"https://github.com/r0gue-io/assets-parachain",
			),
			(
				"r0gue-io/contracts-parachain".to_string(),
				"https://github.com/r0gue-io/contracts-parachain",
			),
			("r0gue-io/evm-parachain".to_string(), "https://github.com/r0gue-io/evm-parachain"),
			// openzeppelin
			(
				"openzeppelin/generic-template".to_string(),
				"https://github.com/OpenZeppelin/polkadot-runtime-templates",
			),
			(
				"openzeppelin/evm-template".to_string(),
				"https://github.com/OpenZeppelin/polkadot-runtime-templates",
			),
			(
				"polkadot-generic-runtime-template".to_string(),
				"https://github.com/OpenZeppelin/polkadot-runtime-templates",
			),
			(
				"paritytech/polkadot-sdk-parachain-template".to_string(),
				"https://github.com/paritytech/polkadot-sdk-parachain-template",
			),
			(
				"paritytech/substrate-contracts-node".to_string(),
				"https://github.com/paritytech/substrate-contracts-node",
			),
			("cpt".to_string(), "https://github.com/paritytech/substrate-contracts-node"),
			("test_01".to_string(), ""),
			("test_02".to_string(), ""),
		])
	}

	fn template_network_configs() -> HashMap<Parachain, Option<&'static str>> {
		[
			(Standard, Some("./network.toml")),
			(Assets, Some("./network.toml")),
			(Contracts, Some("./network.toml")),
			(EVM, Some("./network.toml")),
			(OpenZeppelinGeneric, Some("./zombienet-config/devnet.toml")),
			(OpenZeppelinEVM, Some("./zombienet-config/devnet.toml")),
			(ParityGeneric, Some("./zombienet.toml")),
			(ParityContracts, Some("./zombienet.toml")),
			(TestTemplate01, Some("")),
			(TestTemplate02, Some("")),
		]
		.into()
	}

	fn template_license() -> HashMap<Parachain, Option<&'static str>> {
		[
			(Standard, Some("Unlicense")),
			(Assets, Some("Unlicense")),
			(Contracts, Some("Unlicense")),
			(EVM, Some("Unlicense")),
			(OpenZeppelinGeneric, Some("GPL-3.0")),
			(OpenZeppelinEVM, Some("GPL-3.0")),
			(ParityGeneric, Some("Unlicense")),
			(ParityContracts, Some("Unlicense")),
			(TestTemplate01, Some("Unlicense")),
			(TestTemplate02, Some("GPL-3.0")),
		]
		.into()
	}

	fn template_deployment_name() -> HashMap<Parachain, Option<&'static str>> {
		[
			(Standard, Some("POP_STANDARD")),
			(Assets, Some("POP_ASSETS")),
			(Contracts, Some("POP_CONTRACTS")),
			(EVM, Some("POP_EVM")),
			(OpenZeppelinGeneric, Some("OZ_GENERIC")),
			(OpenZeppelinEVM, Some("OZ_EVM")),
			(ParityGeneric, Some("PARITY_GENERIC")),
			(ParityContracts, None),
			(TestTemplate01, None),
			(TestTemplate02, None),
		]
		.into()
	}

	#[test]
	fn test_is_template_correct() {
		for template in Parachain::VARIANTS {
			if matches!(template, Standard | Assets | Contracts | EVM) {
				assert_eq!(Provider::Pop.provides(&template), true);
				assert_eq!(Provider::Parity.provides(&template), false);
			}
			if matches!(template, ParityContracts | ParityGeneric) {
				assert_eq!(Provider::Pop.provides(&template), false);
				assert_eq!(Provider::Parity.provides(&template), true)
			}
		}
	}

	#[test]
	fn test_convert_string_to_template() {
		let template_names = templates_names();
		// Test the default
		assert_eq!(Parachain::from_str("").unwrap_or_default(), Standard);
		// Test the rest
		for template in Parachain::VARIANTS {
			assert_eq!(
				&Parachain::from_str(&template.to_string()).unwrap(),
				template_names.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_repository_url() {
		let template_urls = templates_urls();
		for template in Parachain::VARIANTS {
			assert_eq!(
				&template.repository_url().unwrap(),
				template_urls.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_network_config() {
		let network_configs = template_network_configs();
		for template in Parachain::VARIANTS {
			println!("{:?}", template.name());
			assert_eq!(template.network_config(), network_configs[template]);
		}
	}

	#[test]
	fn test_license() {
		let licenses = template_license();
		for template in Parachain::VARIANTS {
			assert_eq!(template.license(), licenses[template]);
		}
	}

	#[test]
	fn deployment_name_works() {
		let deployment_name = template_deployment_name();
		for template in Parachain::VARIANTS {
			assert_eq!(template.deployment_name(), deployment_name[template]);
		}
	}

	#[test]
	fn deployment_name_from_based_on_works() {
		for template in Parachain::VARIANTS {
			assert_eq!(
				Parachain::deployment_name_from_based_on(&template.to_string()),
				template.deployment_name().map(String::from),
			);
		}
		// test special cases
		assert_eq!(
			Parachain::deployment_name_from_based_on("OpenZeppelin EVM Template"),
			Some(OpenZeppelinEVM.deployment_name().unwrap().to_string())
		);
		assert_eq!(
			Parachain::deployment_name_from_based_on("OpenZeppelin Generic Template"),
			Some(OpenZeppelinGeneric.deployment_name().unwrap().to_string())
		);
	}

	#[test]
	fn test_default_template_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.default_template(), Some(Standard));
		provider = Provider::Parity;
		assert_eq!(provider.default_template(), Some(ParityGeneric));
	}

	#[test]
	fn test_templates_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.templates(), [&Standard, &Assets, &Contracts, &EVM]);
		provider = Provider::Parity;
		assert_eq!(provider.templates(), [&ParityGeneric, &ParityContracts]);
	}

	#[test]
	fn test_convert_string_to_provider() {
		assert_eq!(Provider::from_str("Pop").unwrap(), Provider::Pop);
		assert_eq!(Provider::from_str("").unwrap_or_default(), Provider::Pop);
		assert_eq!(Provider::from_str("Parity").unwrap(), Provider::Parity);
	}

	#[test]
	fn supported_versions_have_no_whitespace() {
		for template in Parachain::VARIANTS {
			if let Some(versions) = template.supported_versions() {
				for version in versions {
					assert!(!version.contains(' '));
				}
			}
		}
	}

	#[test]
	fn test_supported_versions_works() {
		let template = TestTemplate01;
		assert_eq!(template.supported_versions(), Some(vec!["v1.0.0", "v2.0.0"]));
		assert_eq!(template.is_supported_version("v1.0.0"), true);
		assert_eq!(template.is_supported_version("v2.0.0"), true);
		assert_eq!(template.is_supported_version("v3.0.0"), false);

		let template = TestTemplate02;
		assert_eq!(template.supported_versions(), None);
		// will be true because an empty SupportedVersions defaults to all
		assert_eq!(template.is_supported_version("v1.0.0"), true);
	}

	#[test]
	fn test_is_audited() {
		let template = TestTemplate01;
		assert_eq!(template.is_audited(), true);

		let template = TestTemplate02;
		assert_eq!(template.is_audited(), false);
	}

	#[test]
	fn is_deprecated_works() {
		let template = TestTemplate01;
		assert_eq!(template.is_deprecated(), true);

		let template = TestTemplate02;
		assert_eq!(template.is_deprecated(), false);
	}

	#[test]
	fn deprecated_message_works() {
		let template = TestTemplate01;
		assert_eq!(
			template.deprecated_message(),
			"This template is deprecated. Please use test_02 in the future."
		);

		let template = TestTemplate02;
		assert_eq!(template.deprecated_message(), "");
	}

	#[test]
	fn template_name_without_provider() {
		let template_names = templates_names_without_providers();
		for template in Parachain::VARIANTS {
			assert_eq!(
				template.template_name_without_provider(),
				template_names.get(template).unwrap()
			);
		}
	}
}
