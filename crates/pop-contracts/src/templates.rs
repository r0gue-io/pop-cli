// SPDX-License-Identifier: GPL-3.0

// pub to ease downstream imports
pub use pop_common::templates::{Template, Type};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

// Temporary branch name for v6 contract templates.
pub(crate) const V6_CONTRACTS_BRANCH: &str = "v6.x";

/// Supported contract template providers.
#[derive(
	AsRefStr, Clone, Default, Debug, Display, EnumMessage, EnumString, Eq, PartialEq, VariantArray,
)]
pub enum ContractType {
	/// Contract examples for ink!.
	#[default]
	#[strum(
		ascii_case_insensitive,
		serialize = "examples",
		message = "Examples",
		detailed_message = "Contract examples for ink!."
	)]
	Examples,
	/// ERC-based contracts in ink!.
	#[strum(
		ascii_case_insensitive,
		serialize = "erc",
		message = "ERC",
		detailed_message = "ERC-based contracts in ink!."
	)]
	Erc,
	/// PSP-based contracts in ink!.
	#[strum(
		ascii_case_insensitive,
		serialize = "psp",
		message = "PSP",
		detailed_message = "PSP-based contracts in ink!."
	)]
	Psp,
}

impl Type<Contract> for ContractType {
	fn default_template(&self) -> Option<Contract> {
		match &self {
			ContractType::Examples => Some(Contract::Standard),
			ContractType::Erc => Some(Contract::ERC20),
			ContractType::Psp => Some(Contract::PSP22),
		}
	}
}

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
	/// The implementation of the PSP22 standard in ink!
	#[strum(
		serialize = "PSP22",
		message = "Psp22",
		detailed_message = "The implementation of the PSP22 standard in ink!",
		props(Type = "PSP", Repository = "https://github.com/r0gue-io/PSP22")
	)]
	PSP22,
	/// The implementation of the PSP22 standard in ink!
	#[strum(
		serialize = "PSP34",
		message = "Psp34",
		detailed_message = "The implementation of the PSP34 standard in ink!",
		props(Type = "PSP", Repository = "https://github.com/r0gue-io/PSP34")
	)]
	PSP34,
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
	use std::{collections::HashMap, str::FromStr};
	use strum::VariantArray;
	use Contract::*;

	fn templates_names() -> HashMap<String, Contract> {
		HashMap::from([
			("standard".to_string(), Standard),
			("erc20".to_string(), ERC20),
			("erc721".to_string(), ERC721),
			("erc1155".to_string(), ERC1155),
			("PSP22".to_string(), PSP22),
			("PSP34".to_string(), PSP34),
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
			("PSP22".to_string(), "https://github.com/r0gue-io/PSP22"),
			("PSP34".to_string(), "https://github.com/r0gue-io/PSP34"),
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
			(PSP22, "The implementation of the PSP22 standard in ink!"),
			(PSP34, "The implementation of the PSP34 standard in ink!"),
			(DNS, "Domain name service example implemented in ink!"),
			(CrossContract, "Cross-contract call example implemented in ink!"),
			(Multisig, "Multisig contract example implemented in ink!"),
		])
	}

	#[test]
	fn test_is_template_correct() {
		for template in Contract::VARIANTS {
			if matches!(template, Standard | DNS | CrossContract | Multisig) {
				assert_eq!(ContractType::Examples.provides(template), true);
				assert_eq!(ContractType::Erc.provides(template), false);
				assert_eq!(ContractType::Psp.provides(template), false);
			}
			if matches!(template, ERC20 | ERC721 | ERC1155) {
				assert_eq!(ContractType::Examples.provides(template), false);
				assert_eq!(ContractType::Erc.provides(template), true);
				assert_eq!(ContractType::Psp.provides(template), false);
			}
			if matches!(template, PSP22 | PSP34) {
				assert_eq!(ContractType::Examples.provides(template), false);
				assert_eq!(ContractType::Erc.provides(template), false);
				assert_eq!(ContractType::Psp.provides(template), true);
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
		contract_type = ContractType::Psp;
		assert_eq!(contract_type.default_template(), Some(PSP22));
	}

	#[test]
	fn test_templates_of_type() {
		let mut contract_type = ContractType::Examples;
		assert_eq!(contract_type.templates(), [&Standard, &DNS, &CrossContract, &Multisig]);
		contract_type = ContractType::Erc;
		assert_eq!(contract_type.templates(), [&ERC20, &ERC721, &ERC1155]);
		contract_type = ContractType::Psp;
		assert_eq!(contract_type.templates(), [&PSP22, &PSP34]);
	}

	#[test]
	fn test_convert_string_to_type() {
		assert_eq!(ContractType::from_str("examples").unwrap(), ContractType::Examples);
		assert_eq!(ContractType::from_str("erc").unwrap_or_default(), ContractType::Erc);
		assert_eq!(ContractType::from_str("psp").unwrap_or_default(), ContractType::Psp);
	}
}
