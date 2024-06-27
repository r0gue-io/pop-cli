// SPDX-License-Identifier: GPL-3.0

// pub to ease downstream imports
pub use pop_common::templates::{Template, TemplateType};
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

impl TemplateType<ContractTemplate> for ContractType {
	fn default_template(&self) -> ContractTemplate {
		match &self {
			ContractType::Examples => ContractTemplate::Standard,
			ContractType::Erc => ContractTemplate::ERC20,
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
pub enum ContractTemplate {
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
		props(Type = "ERC", Repository = "https://github.com/use-ink/ink-examples")
	)]
	ERC20,
	/// The implementation of the ERC-721 standard in ink!
	#[strum(
		serialize = "erc721",
		message = "Erc721",
		detailed_message = "The implementation of the ERC-721 standard in ink!",
		props(Type = "ERC", Repository = "https://github.com/use-ink/ink-examples")
	)]
	ERC721,
	/// The implementation of the ERC-1155 standard in ink!
	#[strum(
		serialize = "erc1155",
		message = "Erc1155",
		detailed_message = "The implementation of the ERC-1155 standard in ink!",
		props(Type = "ERC", Repository = "https://github.com/use-ink/ink-examples")
	)]
	ERC1155,
}

impl Template for ContractTemplate {}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{collections::HashMap, str::FromStr};
	use strum::VariantArray;

	fn templates_names() -> HashMap<String, ContractTemplate> {
		HashMap::from([
			("standard".to_string(), ContractTemplate::Standard),
			("erc20".to_string(), ContractTemplate::ERC20),
			("erc721".to_string(), ContractTemplate::ERC721),
			("erc1155".to_string(), ContractTemplate::ERC1155),
		])
	}

	fn templates_urls() -> HashMap<String, &'static str> {
		HashMap::from([
			("erc20".to_string(), "https://github.com/use-ink/ink-examples"),
			("erc721".to_string(), "https://github.com/use-ink/ink-examples"),
			("erc1155".to_string(), "https://github.com/use-ink/ink-examples"),
		])
	}

	fn templates_description() -> HashMap<ContractTemplate, &'static str> {
		HashMap::from([
			(ContractTemplate::Standard, "ink!'s 'Hello World': Flipper"),
			(ContractTemplate::ERC20, "The implementation of the ERC-20 standard in ink!"),
			(ContractTemplate::ERC721, "The implementation of the ERC-721 standard in ink!"),
			(ContractTemplate::ERC1155, "The implementation of the ERC-1155 standard in ink!"),
		])
	}

	#[test]
	fn test_is_template_correct() {
		for template in ContractTemplate::VARIANTS {
			if matches!(template, ContractTemplate::Standard) {
				assert_eq!(ContractType::Examples.matches(template), true);
				assert_eq!(ContractType::Erc.matches(template), false);
			}
			if matches!(
				template,
				ContractTemplate::ERC20 | ContractTemplate::ERC721 | ContractTemplate::ERC1155
			) {
				assert_eq!(ContractType::Examples.matches(template), false);
				assert_eq!(ContractType::Erc.matches(template), true);
			}
		}
	}

	#[test]
	fn test_convert_string_to_template() {
		let template_names = templates_names();
		// Test the default
		assert_eq!(ContractTemplate::from_str("").unwrap_or_default(), ContractTemplate::Standard);
		// Test the rest
		for template in ContractTemplate::VARIANTS {
			assert_eq!(
				&ContractTemplate::from_str(&template.to_string()).unwrap(),
				template_names.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_repository_url() {
		let template_urls = templates_urls();
		for template in ContractTemplate::VARIANTS {
			if matches!(template, ContractTemplate::Standard) {
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
		for template in ContractTemplate::VARIANTS {
			assert_eq!(template.description(), templates_description[template]);
		}
	}

	#[test]
	fn test_default_template_of_type() {
		let mut contract_type = ContractType::Examples;
		assert_eq!(contract_type.default_template(), ContractTemplate::Standard);
		contract_type = ContractType::Erc;
		assert_eq!(contract_type.default_template(), ContractTemplate::ERC20);
	}

	#[test]
	fn test_templates_of_type() {
		let mut contract_type = ContractType::Examples;
		assert_eq!(contract_type.templates(), [&ContractTemplate::Standard]);
		contract_type = ContractType::Erc;
		assert_eq!(
			contract_type.templates(),
			[&ContractTemplate::ERC20, &ContractTemplate::ERC721, &ContractTemplate::ERC1155]
		);
	}

	#[test]
	fn test_convert_string_to_type() {
		assert_eq!(ContractType::from_str("Examples").unwrap(), ContractType::Examples);
		assert_eq!(ContractType::from_str("Erc").unwrap_or_default(), ContractType::Erc);
	}
}
