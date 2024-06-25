// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use strum::{
	EnumMessage as EnumMessageT, EnumProperty as EnumPropertyT, VariantArray as VariantArrayT,
};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

/// Supported template providers.
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

impl ContractType {
	/// Get the list of providers supported.
	pub fn types() -> &'static [ContractType] {
		ContractType::VARIANTS
	}

	/// Get provider's name.
	pub fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the default template of the provider.
	pub fn default_type(&self) -> Template {
		match &self {
			ContractType::Examples => Template::Standard,
			ContractType::Erc => Template::ERC20,
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
			.filter(|t| t.get_str("ContractType") == Some(self.name()))
			.collect()
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
pub enum Template {
	/// A minimalist contract template.
	#[default]
	#[strum(
		serialize = "standard",
		message = "Standard",
		detailed_message = "ink!'s 'Hello World': Flipper",
		props(ContractType = "Examples")
	)]
	Standard,
	/// The implementation of the ERC-20 standard in ink!
	#[strum(
		serialize = "erc20",
		message = "Erc20",
		detailed_message = "The implementation of the ERC-20 standard in ink!",
		props(ContractType = "ERC", Repository = "https://github.com/use-ink/ink-examples")
	)]
	ERC20,
	/// The implementation of the ERC-721 standard in ink!
	#[strum(
		serialize = "erc721",
		message = "Erc721",
		detailed_message = "The implementation of the ERC-721 standard in ink!",
		props(ContractType = "ERC", Repository = "https://github.com/use-ink/ink-examples")
	)]
	ERC721,
	/// The implementation of the ERC-1155 standard in ink!
	#[strum(
		serialize = "erc1155",
		message = "Erc1155",
		detailed_message = "The implementation of the ERC-1155 standard in ink!",
		props(ContractType = "ERC", Repository = "https://github.com/use-ink/ink-examples")
	)]
	ERC1155,
}

impl Template {
	/// Get the template's name.
	pub fn name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the description of the template.
	pub fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the template's repository url.
	pub fn repository_url(&self) -> Result<&str, Error> {
		self.get_str("Repository").ok_or(Error::RepositoryMissing)
	}
	/// Get the list of supported templates.
	pub fn templates() -> &'static [Template] {
		Template::VARIANTS
	}

	/// Check the template belongs to a `provider`.
	pub fn matches(&self, contract_type: &ContractType) -> bool {
		// Match explicitly on provider name (message)
		self.get_str("ContractType") == Some(contract_type.name())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{collections::HashMap, str::FromStr};

	fn templates_names() -> HashMap<String, Template> {
		HashMap::from([
			("standard".to_string(), Template::Standard),
			("erc20".to_string(), Template::ERC20),
			("erc721".to_string(), Template::ERC721),
			("erc1155".to_string(), Template::ERC1155),
		])
	}

	fn templates_urls() -> HashMap<String, &'static str> {
		HashMap::from([
			("erc20".to_string(), "https://github.com/use-ink/ink-examples"),
			("erc721".to_string(), "https://github.com/use-ink/ink-examples"),
			("erc1155".to_string(), "https://github.com/use-ink/ink-examples"),
		])
	}

	fn templates_description() -> HashMap<Template, &'static str> {
		HashMap::from([
			(Template::Standard, "ink!'s 'Hello World': Flipper"),
			(Template::ERC20, "The implementation of the ERC-20 standard in ink!"),
			(Template::ERC721, "The implementation of the ERC-721 standard in ink!"),
			(Template::ERC1155, "The implementation of the ERC-1155 standard in ink!"),
		])
	}

	#[test]
	fn test_is_template_correct() {
		for template in Template::VARIANTS {
			if matches!(template, Template::Standard) {
				assert_eq!(template.matches(&ContractType::Examples), true);
				assert_eq!(template.matches(&ContractType::Erc), false);
			}
			if matches!(template, Template::ERC20 | Template::ERC721 | Template::ERC1155) {
				assert_eq!(template.matches(&ContractType::Examples), false);
				assert_eq!(template.matches(&ContractType::Erc), true);
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
			if matches!(template, Template::Standard) {
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
		for template in Template::VARIANTS {
			assert_eq!(template.description(), templates_description[template]);
		}
	}

	#[test]
	fn test_default_template_of_type() {
		let mut contract_type = ContractType::Examples;
		assert_eq!(contract_type.default_type(), Template::Standard);
		contract_type = ContractType::Erc;
		assert_eq!(contract_type.default_type(), Template::ERC20);
	}

	#[test]
	fn test_templates_of_type() {
		let mut provider = ContractType::Examples;
		assert_eq!(provider.templates(), [&Template::Standard]);
		provider = ContractType::Erc;
		assert_eq!(provider.templates(), [&Template::ERC20, &Template::ERC721, &Template::ERC1155]);
	}

	#[test]
	fn test_convert_string_to_type() {
		assert_eq!(ContractType::from_str("Examples").unwrap(), ContractType::Examples);
		assert_eq!(ContractType::from_str("Erc").unwrap_or_default(), ContractType::Erc);
	}
}
