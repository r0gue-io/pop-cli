// SPDX-License-Identifier: GPL-3.0

// pub to ease downstream imports
pub use pop_common::templates::Template;
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

/// A smart contract template.
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
	/// Domain name service example implemented in ink!
	#[strum(
		serialize = "dns",
		message = "DNS",
		detailed_message = "Domain name service example implemented in ink!",
		props(Type = "Examples", Repository = "https://github.com/use-ink/ink-examples")
	)]
	DNS,
	/// Cross-contract call example implemented in ink!
	#[strum(
		serialize = "cross-contract-calls",
		message = "Cross Contract Calls",
		detailed_message = "Cross-contract call example implemented in ink!",
		props(Type = "Examples", Repository = "https://github.com/use-ink/ink-examples")
	)]
	CrossContract,
	/// Multisig contract example implemented in ink!
	#[strum(
		serialize = "multisig",
		message = "Multisig Contract",
		detailed_message = "Multisig contract example implemented in ink!",
		props(Type = "Examples", Repository = "https://github.com/use-ink/ink-examples")
	)]
	Multisig,
}

impl Template for Contract {}

#[cfg(test)]
mod tests {
	use super::*;
	use Contract::*;
	use std::{collections::HashMap, str::FromStr};
	use strum::VariantArray;

	fn templates_names() -> HashMap<String, Contract> {
		HashMap::from([
			("standard".to_string(), Standard),
			("erc20".to_string(), ERC20),
			("erc721".to_string(), ERC721),
			("erc1155".to_string(), ERC1155),
			("dns".to_string(), DNS),
			("cross-contract-calls".to_string(), CrossContract),
			("multisig".to_string(), Multisig),
		])
	}

	fn templates_urls() -> HashMap<String, &'static str> {
		HashMap::from([
			("erc20".to_string(), "https://github.com/use-ink/ink-examples"),
			("erc721".to_string(), "https://github.com/use-ink/ink-examples"),
			("erc1155".to_string(), "https://github.com/use-ink/ink-examples"),
			("dns".to_string(), "https://github.com/use-ink/ink-examples"),
			("cross-contract-calls".to_string(), "https://github.com/use-ink/ink-examples"),
			("multisig".to_string(), "https://github.com/use-ink/ink-examples"),
		])
	}

	fn templates_description() -> HashMap<Contract, &'static str> {
		HashMap::from([
			(Standard, "ink!'s 'Hello World': Flipper"),
			(ERC20, "The implementation of the ERC-20 standard in ink!"),
			(ERC721, "The implementation of the ERC-721 standard in ink!"),
			(ERC1155, "The implementation of the ERC-1155 standard in ink!"),
			(DNS, "Domain name service example implemented in ink!"),
			(CrossContract, "Cross-contract call example implemented in ink!"),
			(Multisig, "Multisig contract example implemented in ink!"),
		])
	}

	#[test]
	fn test_convert_string_to_template() {
		let template_names = templates_names();
		// Test the default
		assert_eq!(Contract::from_str("").unwrap_or_default(), Standard);
		// Test the rest
		for template in Contract::VARIANTS {
			assert_eq!(
				&Contract::from_str(template.as_ref()).unwrap(),
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
}
