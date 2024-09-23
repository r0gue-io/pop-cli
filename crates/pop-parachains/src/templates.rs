// SPDX-License-Identifier: GPL-3.0

use pop_common::templates::{Template, Type};
use strum::EnumProperty as _;
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

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
	#[strum(
		ascii_case_insensitive,
		serialize = "tanssi",
		message = "Tanssi",
		detailed_message = "Swift and effortless deployment of application-specific blockchains."
	)]
	Tanssi,
}

impl Type<Parachain> for Provider {
	fn default_template(&self) -> Option<Parachain> {
		match &self {
			Provider::Pop => Some(Parachain::Standard),
			Provider::OpenZeppelin => Some(Parachain::OpenZeppelinGeneric),
			Provider::Parity => Some(Parachain::ParityContracts),
			Provider::Tanssi => Some(Parachain::TanssiSimple),
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
pub enum Parachain {
	/// Minimalist parachain template.
	#[default]
	#[strum(
		serialize = "standard",
		message = "Standard",
		detailed_message = "A standard parachain",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/base-parachain",
			Network = "./network.toml",
			License = "Unlicense"
		)
	)]
	Standard,
	/// Parachain configured with fungible and non-fungilble asset functionalities.
	#[strum(
		serialize = "assets",
		message = "Assets",
		detailed_message = "Parachain configured with fungible and non-fungilble asset \
		                    functionalities.",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/assets-parachain",
			Network = "./network.toml",
			License = "Unlicense"
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
			Network = "./network.toml",
			License = "Unlicense"
		)
	)]
	Contracts,
	/// Parachain configured with Frontier, enabling compatibility with the Ethereum Virtual
	/// Machine (EVM).
	#[strum(
		serialize = "evm",
		message = "EVM",
		detailed_message = "Parachain configured with Frontier, enabling compatibility with the \
		                    Ethereum Virtual Machine (EVM).",
		props(
			Provider = "Pop",
			Repository = "https://github.com/r0gue-io/evm-parachain",
			Network = "./network.toml",
			License = "Unlicense"
		)
	)]
	EVM,
	/// OpenZeppelin generic template.
	#[strum(
		serialize = "polkadot-generic-runtime-template",
		message = "Generic Runtime Template",
		detailed_message = "A generic template for Substrate Runtime",
		props(
			Provider = "OpenZeppelin",
			Repository = "https://github.com/OpenZeppelin/polkadot-runtime-templates",
			Network = "./zombienet-config/devnet.toml",
			SupportedVersions = "v1.0.0",
			IsAudited = "true",
			License = "GPL-3.0"
		)
	)]
	OpenZeppelinGeneric,
	/// Minimal Substrate node configured for smart contracts via pallet-contracts.
	#[strum(
		serialize = "cpt",
		message = "Contracts",
		detailed_message = "Minimal Substrate node configured for smart contracts via \
		                    pallet-contracts.",
		props(
			Provider = "Parity",
			Repository = "https://github.com/paritytech/substrate-contracts-node",
			Network = "./zombienet.toml",
			License = "Unlicense"
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
			Network = "./zombienet-config.toml",
			License = "Unlicense"
		)
	)]
	ParityFPT,
	/// Tanssi generic container chain template.
	#[strum(
		serialize = "tanssi/simple",
		message = "Simple",
		detailed_message = "Generic container chain template.",
		props(
			Provider = "Tanssi",
			Repository = "https://github.com/moondance-labs/tanssi",
			Network = "./network.toml",
			NodeDirectory = "container-chains/nodes",
			RuntimeDirectory = "container-chains/runtime-templates",
			ExtractableFiles = ".gitignore,.rustfmt.toml,Cargo.toml,Cargo.lock,LICENSE,README.md,rust-toolchain",
		)
	)]
	TanssiSimple,

	// Templates for unit tests below.
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
			NodeDirectory = "node/dir",
			RuntimeDirectory = "runtime/dir",
			ExtractableFiles = "list,of,files",
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

	/// Returns a list supported versions for a concrete template, if defined.
	pub fn supported_versions(&self) -> Option<Vec<&str>> {
		self.get_str("SupportedVersions").map(|s| s.split(',').collect())
	}

	/// Whether a certain version is supported for the template.
	pub fn is_supported_version(&self, version: &str) -> bool {
		// if `SupportedVersion` is None, then all versions are supported. Otherwise, ensure version
		// is present.
		self.supported_versions().map_or(true, |versions| versions.contains(&version))
	}

	/// Whether a concrete template is audited of not.
	pub fn is_audited(&self) -> bool {
		self.get_str("IsAudited").map_or(false, |s| s == "true")
	}

	pub fn license(&self) -> Option<&str> {
		self.get_str("License")
	}

	/// Gets the template name, removing the provider if present.
	pub fn template_name_without_provider(&self) -> &str {
		let name = self.as_ref();
		name.split_once('/').map_or(name, |(_, template)| template)
	}

	/// Returns where to pull the template node directory from in its source repo, if defined.
	pub fn node_directory(&self) -> Option<&str> {
		self.get_str("NodeDirectory")
	}

	/// Returns where to pull the template runtime directory from in its source repo, if defined.
	pub fn runtime_directory(&self) -> Option<&str> {
		self.get_str("RuntimeDirectory")
	}

	/// Returns a list of relevant files to extract from the source repo, if defined.
	pub fn extractable_files(&self) -> Option<Vec<&str>> {
		self.get_str("ExtractableFiles").map(|s| s.split(',').collect())
	}
}

#[cfg(test)]
mod tests {
	use std::{collections::HashMap, str::FromStr};

	use strum::VariantArray;
	use Parachain::*;

	use super::*;

	fn templates_names() -> HashMap<String, Parachain> {
		HashMap::from([
			// Pop.
			("standard".to_string(), Standard),
			("assets".to_string(), Assets),
			("contracts".to_string(), Contracts),
			("evm".to_string(), EVM),
			// OpenZeppelin.
			("polkadot-generic-runtime-template".to_string(), OpenZeppelinGeneric),
			// Parity.
			("cpt".to_string(), ParityContracts),
			("fpt".to_string(), ParityFPT),
			// Tanssi.
			("tanssi/simple".to_string(), TanssiSimple),
			// Test.
			("test_01".to_string(), TestTemplate01),
			("test_02".to_string(), TestTemplate02),
		])
	}

	fn templates_names_without_providers() -> HashMap<Parachain, String> {
		HashMap::from([
			(Standard, "standard".to_string()),
			(Assets, "assets".to_string()),
			(Contracts, "contracts".to_string()),
			(EVM, "evm".to_string()),
			(OpenZeppelinGeneric, "polkadot-generic-runtime-template".to_string()),
			(TanssiSimple, "simple".to_string()),
			(ParityContracts, "cpt".to_string()),
			(ParityFPT, "fpt".to_string()),
			(TestTemplate01, "test_01".to_string()),
			(TestTemplate02, "test_02".to_string()),
		])
	}

	fn templates_urls() -> HashMap<String, &'static str> {
		HashMap::from([
			// Pop.
			("standard".to_string(), "https://github.com/r0gue-io/base-parachain"),
			("assets".to_string(), "https://github.com/r0gue-io/assets-parachain"),
			("contracts".to_string(), "https://github.com/r0gue-io/contracts-parachain"),
			("evm".to_string(), "https://github.com/r0gue-io/evm-parachain"),
			// OpenZeppelin.
			(
				"polkadot-generic-runtime-template".to_string(),
				"https://github.com/OpenZeppelin/polkadot-runtime-templates",
			),
			// Parity.
			("cpt".to_string(), "https://github.com/paritytech/substrate-contracts-node"),
			("fpt".to_string(), "https://github.com/paritytech/frontier-parachain-template"),
			// Tanssi.
			("tanssi/simple".to_string(), "https://github.com/moondance-labs/tanssi"),
			// Test.
			("test_01".to_string(), ""),
			("test_02".to_string(), ""),
		])
	}

	fn template_network_configs() -> HashMap<Parachain, Option<&'static str>> {
		[
			// Pop.
			(Standard, Some("./network.toml")),
			(Assets, Some("./network.toml")),
			(Contracts, Some("./network.toml")),
			(EVM, Some("./network.toml")),
			// OpenZeppelin.
			(OpenZeppelinGeneric, Some("./zombienet-config/devnet.toml")),
			// Parity.
			(ParityContracts, Some("./zombienet.toml")),
			(ParityFPT, Some("./zombienet-config.toml")),
			// Tanssi.
			(TanssiSimple, Some("./network.toml")),
			// Test.
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
			(ParityContracts, Some("Unlicense")),
			(ParityFPT, Some("Unlicense")),
			(TestTemplate01, Some("Unlicense")),
			(TestTemplate02, Some("GPL-3.0")),
		]
		.into()
	}

	#[test]
	fn test_is_template_correct() {
		for template in Parachain::VARIANTS {
			if matches!(template, Standard | Assets | Contracts | EVM) {
				assert_eq!(Provider::Pop.provides(&template), true);
				assert_eq!(Provider::Parity.provides(&template), false);
				assert_eq!(Provider::OpenZeppelin.provides(&template), false);
				assert_eq!(Provider::Tanssi.provides(&template), false);
			}
			if matches!(template, ParityContracts | ParityFPT) {
				assert_eq!(Provider::Pop.provides(&template), false);
				assert_eq!(Provider::Parity.provides(&template), true);
				assert_eq!(Provider::OpenZeppelin.provides(&template), false);
				assert_eq!(Provider::Tanssi.provides(&template), false);
			}
			if matches!(template, OpenZeppelinGeneric) {
				assert_eq!(Provider::Pop.provides(&template), false);
				assert_eq!(Provider::Parity.provides(&template), false);
				assert_eq!(Provider::OpenZeppelin.provides(&template), true);
				assert_eq!(Provider::Tanssi.provides(&template), false);
			}
			if matches!(template, TanssiSimple) {
				assert_eq!(Provider::Pop.provides(&template), false);
				assert_eq!(Provider::Parity.provides(&template), false);
				assert_eq!(Provider::OpenZeppelin.provides(&template), false);
				assert_eq!(Provider::Tanssi.provides(&template), true);
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
	fn test_default_template_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.default_template(), Some(Standard));
		provider = Provider::Parity;
		assert_eq!(provider.default_template(), Some(ParityContracts));
	}

	#[test]
	fn test_templates_of_provider() {
		let mut provider = Provider::Pop;
		assert_eq!(provider.templates(), [&Standard, &Assets, &Contracts, &EVM]);
		provider = Provider::Parity;
		assert_eq!(provider.templates(), [&ParityContracts, &ParityFPT]);
	}

	#[test]
	fn test_convert_string_to_provider() {
		assert_eq!(Provider::from_str("Pop").unwrap(), Provider::Pop);
		assert_eq!(Provider::from_str("").unwrap_or_default(), Provider::Pop);
		assert_eq!(Provider::from_str("Parity").unwrap(), Provider::Parity);
		assert_eq!(Provider::from_str("OpenZeppelin").unwrap_or_default(), Provider::OpenZeppelin);
		assert_eq!(Provider::from_str("Tanssi").unwrap_or_default(), Provider::Tanssi);
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
	fn retrieves_node_directory_from_template() {
		let template = TestTemplate01;
		assert_eq!(template.node_directory().unwrap(), "node/dir");
		let template = TestTemplate02;
		assert_eq!(template.node_directory(), None);
	}

	#[test]
	fn retrieves_runtime_directory_from_template() {
		let template = TestTemplate01;
		assert_eq!(template.runtime_directory().unwrap(), "runtime/dir");
		let template = TestTemplate02;
		assert_eq!(template.runtime_directory(), None);
	}

	#[test]
	fn extractable_files_from_template_wprk() {
		let template = TestTemplate01;
		assert_eq!(template.extractable_files(), Some(vec!["list", "of", "files"]));
		let template = TestTemplate02;
		assert_eq!(template.supported_versions(), None);
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
