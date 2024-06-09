// SPDX-License-Identifier: GPL-3.0
use strum::{
	EnumMessage as EnumMessageT, EnumProperty as EnumPropertyT, VariantArray as VariantArrayT,
};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

use crate::errors::Error;

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
	#[strum(serialize = "standard", message = "Standard", detailed_message = "Our 'Hello World")]
	Standard,
	/// The implementation of the ERC-20 standard in Solidity using ink!
	#[strum(
		serialize = "erc20",
		message = "Erc20",
		detailed_message = "The implementation of the ERC-20 standard in Solidity using ink!",
		props(Repository = "https://github.com/paritytech/ink-examples")
	)]
	ERC20,
	/// The implementation of the ERC-721 standard in Solidity using ink!
	#[strum(
		serialize = "erc721",
		message = "Erc721",
		detailed_message = "The implementation of the ERC-721 standard in Solidity using ink!",
		props(Repository = "https://github.com/paritytech/ink-examples")
	)]
	ERC721,
	/// The implementation of the ERC-1155 standard in Solidity using ink!
	#[strum(
		serialize = "erc1155",
		message = "Erc1155",
		detailed_message = "The implementation of the ERC-1155 standard in Solidity using ink!",
		props(Repository = "https://github.com/paritytech/ink-examples")
	)]
	ERC1155,
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

	/// Get the template's repository url.
	pub fn repository_url(&self) -> Result<&str, Error> {
		self.get_str("Repository").ok_or(Error::RepositoryMissing)
	}
	/// Get the list of templates supported.
	pub fn templates() -> &'static [Template] {
		Template::VARIANTS
	}
}
