// SPDX-License-Identifier: GPL-3.0

// pub to ease downstream imports
pub use pop_common::templates::{Template, Type};
use strum::EnumProperty as _;
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

/// Supported contract types.
#[derive(
	AsRefStr, Clone, Default, Debug, Display, EnumMessage, EnumString, Eq, PartialEq, VariantArray,
)]
pub enum ContractType {
	#[default]
	#[strum(
		ascii_case_insensitive,
		serialize = "examples",
		message = "Examples",
		detailed_message = "Contract examples for ink!."
	)]
	Examples,
	#[strum(
		ascii_case_insensitive,
		serialize = "erc",
		message = "ERC",
		detailed_message = "ERC-based contracts in ink!."
	)]
	Erc,
}

impl Type<Contract> for ContractType {
	fn default_template(&self) -> Option<Contract> {
		match &self {
			ContractType::Examples => Some(Contract::Standard),
			ContractType::Erc => Some(Contract::ERC20),
		}
	}
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
	Hash,
	PartialEq,
	VariantArray,
)]
pub enum Contract {
	/// A minimalist contract template.
	#[default]
	#[strum(
		serialize = "standard",
		message = "Standard",
		detailed_message = "ink!'s 'Hello World': Flipper",
		props(Type = "Examples")
	)]
	Standard,
	/// The implementation of the ERC-20 standard in ink!
	#[strum(
		serialize = "erc20",
		message = "Erc20",
		detailed_message = "The implementation of the ERC-20 standard in ink!",
		props(
			Type = "ERC",
			Repository = "https://github.com/use-ink/ink-examples/archive/refs/heads/main.tar.gz",
			Archive = "ink-examples-main"
		)
	)]
	ERC20,
	/// The implementation of the ERC-721 standard in ink!
	#[strum(
		serialize = "erc721",
		message = "Erc721",
		detailed_message = "The implementation of the ERC-721 standard in ink!",
		props(
			Type = "ERC",
			Repository = "https://github.com/use-ink/ink-examples/archive/refs/heads/main.tar.gz",
			Archive = "ink-examples-main"
		)
	)]
	ERC721,
	/// The implementation of the ERC-1155 standard in ink!
	#[strum(
		serialize = "erc1155",
		message = "Erc1155",
		detailed_message = "The implementation of the ERC-1155 standard in ink!",
		props(
			Type = "ERC",
			Repository = "https://github.com/use-ink/ink-examples/archive/refs/heads/main.tar.gz",
			Archive = "ink-examples-main"
		)
	)]
	ERC1155,
}

impl Template for Contract {}

impl Contract {
	/// Returns the archive name, if defined.
	pub fn archive_name(&self) -> Option<&str> {
		self.get_str("Archive")
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{collections::HashMap, str::FromStr};
	use strum::VariantArray;
	use Contract::*;

	fn templates_names() -> HashMap<String, Contract> {
		HashMap::from([
			("standard".to_string(), Standard),
			("erc20".to_string(), ERC20),
			("erc721".to_string(), ERC721),
			("erc1155".to_string(), ERC1155),
		])
	}

	fn templates_urls() -> HashMap<String, &'static str> {
		HashMap::from([
			(
				"erc20".to_string(),
				"https://github.com/use-ink/ink-examples/archive/refs/heads/main.tar.gz",
			),
			(
				"erc721".to_string(),
				"https://github.com/use-ink/ink-examples/archive/refs/heads/main.tar.gz",
			),
			(
				"erc1155".to_string(),
				"https://github.com/use-ink/ink-examples/archive/refs/heads/main.tar.gz",
			),
		])
	}

	fn templates_description() -> HashMap<Contract, &'static str> {
		HashMap::from([
			(Standard, "ink!'s 'Hello World': Flipper"),
			(ERC20, "The implementation of the ERC-20 standard in ink!"),
			(ERC721, "The implementation of the ERC-721 standard in ink!"),
			(ERC1155, "The implementation of the ERC-1155 standard in ink!"),
		])
	}

	fn template_archive_names() -> HashMap<Contract, Option<&'static str>> {
		[
			(Contract::Standard, None),
			(Contract::ERC20, Some("ink-examples-main")),
			(Contract::ERC721, Some("ink-examples-main")),
			(Contract::ERC1155, Some("ink-examples-main")),
		]
		.into()
	}

	#[test]
	fn test_is_template_correct() {
		for template in Contract::VARIANTS {
			if matches!(template, Standard) {
				assert_eq!(ContractType::Examples.provides(template), true);
				assert_eq!(ContractType::Erc.provides(template), false);
			}
			if matches!(template, ERC20 | ERC721 | ERC1155) {
				assert_eq!(ContractType::Examples.provides(template), false);
				assert_eq!(ContractType::Erc.provides(template), true);
			}
		}
	}

	#[test]
	fn test_convert_string_to_template() {
		let template_names = templates_names();
		// Test the default
		assert_eq!(Contract::from_str("").unwrap_or_default(), Standard);
		// Test the rest
		for template in Contract::VARIANTS {
			assert_eq!(
				&Contract::from_str(&template.to_string()).unwrap(),
				template_names.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_repository_url() {
		let template_urls = templates_urls();
		for template in Contract::VARIANTS {
			if matches!(template, Standard) {
				assert!(&template.repository_url().is_err());
			} else {
				assert_eq!(
					&template.repository_url().unwrap(),
					template_urls.get(&template.to_string()).unwrap()
				);
			}
		}
	}

	#[test]
	fn test_templates_description() {
		let templates_description = templates_description();
		for template in Contract::VARIANTS {
			assert_eq!(template.description(), templates_description[template]);
		}
	}

	#[test]
	fn test_default_template_of_type() {
		let mut contract_type = ContractType::Examples;
		assert_eq!(contract_type.default_template(), Some(Standard));
		contract_type = ContractType::Erc;
		assert_eq!(contract_type.default_template(), Some(ERC20));
	}

	#[test]
	fn test_templates_of_type() {
		let mut contract_type = ContractType::Examples;
		assert_eq!(contract_type.templates(), [&Standard]);
		contract_type = ContractType::Erc;
		assert_eq!(contract_type.templates(), [&ERC20, &ERC721, &ERC1155]);
	}

	#[test]
	fn test_convert_string_to_type() {
		assert_eq!(ContractType::from_str("Examples").unwrap(), ContractType::Examples);
		assert_eq!(ContractType::from_str("Erc").unwrap_or_default(), ContractType::Erc);
	}

	#[test]
	fn test_archive_name() {
		let archive_names = template_archive_names();
		for template in Contract::VARIANTS {
			assert_eq!(template.archive_name(), archive_names[template]);
		}
	}
}
