// SPDX-License-Identifier: GPL-3.0

// pub to ease downstream imports
pub use pop_common::templates::{Template, Type};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

/// Supported contract tempalte providers.
#[derive(
	AsRefStr, Clone, Default, Debug, Display, EnumMessage, EnumString, Eq, PartialEq, VariantArray,
)]
pub enum ContractProvider {
	#[default]
	#[strum(
		ascii_case_insensitive,
		serialize = "useink",
		message = "UseInk",
		detailed_message = "Contract examples for ink!."
	)]
	UseInk,
	#[strum(
		ascii_case_insensitive,
		serialize = "cardinal",
		message = "CardinalCryptography",
		detailed_message = "Developers of Aleph Zero."
	)]
	CardinalCryptography,
}

impl Type<Contract> for ContractProvider {
	fn default_template(&self) -> Option<Contract> {
		match &self {
			ContractProvider::UseInk => Some(Contract::Standard),
			ContractProvider::CardinalCryptography => Some(Contract::PSP22),
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
		props(Provider = "UseInk", Type = "Examples")
	)]
	Standard,
	/// The implementation of the ERC-20 standard in ink!
	#[strum(
		serialize = "erc20",
		message = "Erc20",
		detailed_message = "The implementation of the ERC-20 standard in ink!",
		props(
			Provider = "UseInk",
			Type = "ERC",
			Repository = "https://github.com/use-ink/ink-examples"
		)
	)]
	ERC20,
	/// The implementation of the ERC-721 standard in ink!
	#[strum(
		serialize = "erc721",
		message = "Erc721",
		detailed_message = "The implementation of the ERC-721 standard in ink!",
		props(
			Provider = "UseInk",
			Type = "ERC",
			Repository = "https://github.com/use-ink/ink-examples"
		)
	)]
	ERC721,
	/// The implementation of the ERC-1155 standard in ink!
	#[strum(
		serialize = "erc1155",
		message = "Erc1155",
		detailed_message = "The implementation of the ERC-1155 standard in ink!",
		props(
			Provider = "UseInk",
			Type = "ERC",
			Repository = "https://github.com/use-ink/ink-examples"
		)
	)]
	ERC1155,
	/// The implementation of the PSP22 standard in ink!
	#[strum(
		serialize = "PSP22",
		message = "Psp22",
		detailed_message = "The implementation of the PSP22 standard in ink!",
		props(
			Provider = "CardinalCryptography",
			Type = "PSP",
			Repository = "https://github.com/Cardinal-Cryptography/PSP22"
		)
	)]
	PSP22,
	/// The implementation of the PSP22 standard in ink!
	#[strum(
		serialize = "PSP34",
		message = "Psp34",
		detailed_message = "The implementation of the PSP34 standard in ink!",
		props(
			Provider = "CardinalCryptography",
			Type = "PSP",
			Repository = "https://github.com/Cardinal-Cryptography/PSP34"
		)
	)]
	PSP34,
	/// Domain name service example implemented in ink!
	#[strum(
		serialize = "dns",
		message = "DNS",
		detailed_message = "Domain name service example implemented in ink!",
		props(
			Provider = "UseInk",
			Type = "Examples",
			Repository = "https://github.com/use-ink/ink-examples"
		)
	)]
	DNS,
	/// Cross-contract call example implemented in ink!
	#[strum(
		serialize = "cross-contract-calls",
		message = "Cross Contract Calls",
		detailed_message = "Cross-contract call example implemented in ink!",
		props(
			Provider = "UseInk",
			Type = "Examples",
			Repository = "https://github.com/use-ink/ink-examples"
		)
	)]
	CrossContract,
	/// Multisig contract example implemented in ink!
	#[strum(
		serialize = "multisig",
		message = "Multisig Contract",
		detailed_message = "Multisig contract example implemented in ink!",
		props(
			Provider = "UseInk",
			Type = "Examples",
			Repository = "https://github.com/use-ink/ink-examples"
		)
	)]
	Multisig,
}

impl Template for Contract {
	const PROPERTY: &'static str = "Provider";
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
			("PSP22".to_string(), "https://github.com/Cardinal-Cryptography/PSP22"),
			("PSP34".to_string(), "https://github.com/Cardinal-Cryptography/PSP34"),
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
			if matches!(
				template,
				Standard | ERC20 | ERC721 | ERC1155 | DNS | CrossContract | Multisig
			) {
				assert_eq!(ContractProvider::UseInk.provides(template), true);
				assert_eq!(ContractProvider::CardinalCryptography.provides(template), false);
			}
			if matches!(template, PSP22 | PSP34) {
				assert_eq!(ContractProvider::UseInk.provides(template), false);
				assert_eq!(ContractProvider::CardinalCryptography.provides(template), true);
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
		let mut contract_provider = ContractProvider::UseInk;
		assert_eq!(contract_provider.default_template(), Some(Standard));
		contract_provider = ContractProvider::CardinalCryptography;
		assert_eq!(contract_provider.default_template(), Some(PSP22));
	}

	#[test]
	fn test_templates_of_type() {
		let mut contract_provider = ContractProvider::UseInk;
		assert_eq!(
			contract_provider.templates(),
			[&Standard, &ERC20, &ERC721, &ERC1155, &DNS, &CrossContract, &Multisig]
		);
		contract_provider = ContractProvider::CardinalCryptography;
		assert_eq!(contract_provider.templates(), [&PSP22, &PSP34]);
	}

	#[test]
	fn test_convert_string_to_type() {
		assert_eq!(ContractProvider::from_str("useink").unwrap(), ContractProvider::UseInk);
		assert_eq!(
			ContractProvider::from_str("cardinal").unwrap_or_default(),
			ContractProvider::CardinalCryptography
		);
	}
}
