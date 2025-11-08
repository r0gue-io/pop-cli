// SPDX-License-Identifier: GPL-3.0

use strum::{EnumProperty as _, VariantArray};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString};

use crate::templates::{Template, Type};

/// Supported frontend template types.
#[derive(
	AsRefStr, Clone, Default, Debug, Display, EnumMessage, EnumString, Eq, PartialEq, VariantArray,
)]
pub enum FrontendType {
	/// Contract-based frontend templates.
	#[default]
	#[strum(ascii_case_insensitive, serialize = "contract", message = "Contract")]
	Contract,
	/// Chain-based frontend templates.
	#[strum(ascii_case_insensitive, serialize = "chain", message = "Chain")]
	Chain,
}
impl Type<FrontendTemplate> for FrontendType {
	fn default_template(&self) -> Option<FrontendTemplate> {
		match &self {
			FrontendType::Contract => Some(FrontendTemplate::Typink),
			FrontendType::Chain => Some(FrontendTemplate::CreateDotApp),
		}
	}
}

/// Supported frontend templates.
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
pub enum FrontendTemplate {
	/// Typeink template: The ultimate toolkit for dApps development on Polkadot, powered by <https://dedot.dev>!.
	#[default]
	#[strum(
		serialize = "typink",
		message = "Typink",
		detailed_message = "The ultimate toolkit for dApps development on Polkadot, powered by https://dedot.dev",
		props(Command = "create-typink@latest", Type = "Contract",)
	)]
	Typink,
	/// Inkathon template: Next generation full-stack boilerplate for ink! smart contracts running
	/// on PolkaVM.
	#[strum(
		serialize = "inkathon",
		message = "Inkathon",
		detailed_message = "Next generation full-stack boilerplate for ink! smart contracts running on PolkaVM.",
		props(Command = "create-inkathon-app@latest", Type = "Contract",)
	)]
	Inkathon,
	/// Create Dot App template: A command-line interface (CLI) tool designed to streamline the
	/// development process for Polkadot-based decentralized applications (dApps).
	#[strum(
		serialize = "create-dot-app",
		message = "create-dot-app",
		detailed_message = "A command-line interface (CLI) tool designed to streamline the development process for Polkadot-based decentralized applications (dApps)",
		props(Command = "create-dot-app@latest", Type = "Chain",)
	)]
	CreateDotApp,
}

impl Template for FrontendTemplate {}

impl FrontendTemplate {
	/// Get the command to create a new project using this template.
	pub fn command(&self) -> Option<&str> {
		self.get_str("Command")
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use FrontendTemplate::*;
	use std::{collections::HashMap, str::FromStr};
	use strum::VariantArray;

	fn templates_names() -> HashMap<String, FrontendTemplate> {
		HashMap::from([
			("inkathon".to_string(), Inkathon),
			("typink".to_string(), Typink),
			("create-dot-app".to_string(), CreateDotApp),
		])
	}

	fn templates_commands() -> HashMap<String, &'static str> {
		HashMap::from([
			("inkathon".to_string(), "create-inkathon-app@latest"),
			("typink".to_string(), "create-typink@latest"),
			("create-polkadot-dapp".to_string(), "create-polkadot-dapp@latest"),
			("create-dot-app".to_string(), "create-dot-app@latest"),
		])
	}

	fn templates_description() -> HashMap<FrontendTemplate, &'static str> {
		HashMap::from([
			(
				Inkathon,
				"Next generation full-stack boilerplate for ink! smart contracts running on PolkaVM.",
			),
			(
				Typink,
				"The ultimate toolkit for dApps development on Polkadot, powered by https://dedot.dev",
			),
			(
				CreateDotApp,
				"A command-line interface (CLI) tool designed to streamline the development process for Polkadot-based decentralized applications (dApps)",
			),
		])
	}

	#[test]
	fn test_convert_string_to_template() {
		let template_names = templates_names();
		// Test the default
		assert_eq!(FrontendTemplate::from_str("").unwrap_or_default(), Typink);
		// Test the rest
		for template in FrontendTemplate::VARIANTS {
			assert_eq!(
				&FrontendTemplate::from_str(template.as_ref()).unwrap(),
				template_names.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_repository_command() {
		let template_urls = templates_commands();
		for template in FrontendTemplate::VARIANTS {
			assert_eq!(
				&template.command().unwrap(),
				template_urls.get(&template.to_string()).unwrap()
			);
		}
	}

	#[test]
	fn test_templates_description() {
		let templates_description = templates_description();
		for template in FrontendTemplate::VARIANTS {
			assert_eq!(template.description(), templates_description[template]);
		}
	}

	#[test]
	fn test_templates_of_type() {
		let mut frontend_type = FrontendType::Contract;
		assert_eq!(frontend_type.templates(), [&Typink, &Inkathon]);
		frontend_type = FrontendType::Chain;
		assert_eq!(frontend_type.templates(), [&CreateDotApp]);
	}
}
