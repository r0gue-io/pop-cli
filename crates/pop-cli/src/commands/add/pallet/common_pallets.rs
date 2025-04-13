// SPDX-License-Identifier: GPL-3.0
use clap::{
	builder::{PossibleValue, TypedValueParser},
	ValueEnum,
};
use proc_macro2::{Literal, TokenStream};
use semver::Version;
use std::{ffi::OsStr, str::FromStr};
use strum_macros::{EnumIter, EnumMessage};
use syn::{parse_quote, Item};

#[derive(Debug, Copy, Clone, PartialEq, EnumIter, EnumMessage, ValueEnum)]
pub(crate) enum CommonPallets {
	/// Add pallet-balances to your runtime.
	#[strum(message = "balances", detailed_message = "Add pallet-balances to your runtime.")]
	Balances,
	/// Add pallet-contracts to your runtime.
	#[strum(message = "contracts", detailed_message = "Add pallet-contracts to your runtime.")]
	Contracts,
}

impl CommonPallets {
	pub(crate) fn get_crate_name(&self) -> String {
		match self {
			CommonPallets::Balances => "pallet-balances".to_owned(),
			CommonPallets::Contracts => "pallet-contracts".to_owned(),
		}
	}

	pub(crate) fn get_pallet_declaration_construct_runtime(&self) -> TokenStream {
		match self {
			CommonPallets::Balances => parse_quote! { Balances: pallet_balances, },
			CommonPallets::Contracts => parse_quote! { Contracts: pallet_contracts, },
		}
	}

	pub(crate) fn get_pallet_declaration_runtime_module(&self, highest_index: Literal) -> Item {
		match self {
			CommonPallets::Balances => parse_quote! {
			  ///TEMP_DOC
				#[runtime::pallet_index(#highest_index)]
				pub type Balances = pallet_balances;
			},
			CommonPallets::Contracts => parse_quote! {
			  ///TEMP_DOC
				#[runtime::pallet_index(#highest_index)]
				pub type Contracts = pallet_contracts;
			},
		}
	}

	pub(crate) fn get_impl_needed_use_statements(&self) -> Vec<Item> {
		match self {
			CommonPallets::Balances => vec![
				parse_quote!(
					///TEMP_DOC
					use crate::{System, Runtime, RuntimeEvent, RuntimeHoldReason, RuntimeCall};
				),
				parse_quote!(
					use frame_support::{parameter_types, derive_impl};
				),
			],
			CommonPallets::Contracts => vec![
				parse_quote!(
					///TEMP_DOC
					use crate::{
						System, Runtime, Balances, RuntimeEvent, RuntimeHoldReason, RuntimeCall,
					};
				),
				parse_quote!(
					use frame_support::{parameter_types, derive_impl};
				),
			],
		}
	}

	pub(crate) fn get_needed_parameter_types(&self) -> Item {
		match self {
			CommonPallets::Balances => Item::Verbatim(TokenStream::new()),
			CommonPallets::Contracts => parse_quote! {
			  ///TEMP_DOC
			  parameter_types!{
				  pub Schedule: pallet_contracts::Schedule<Runtime> = <pallet_contracts::Schedule<Runtime>>::default();
			  }
			},
		}
	}

	pub(crate) fn get_needed_impl_block(&self) -> Item {
		match self {
			CommonPallets::Balances => parse_quote! {
			  ///TEMP_DOC
			  #[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
			  impl pallet_balances::Config for Runtime{
				type AccountStore = System;
			  }
			},
			CommonPallets::Contracts => parse_quote! {
			  ///TEMP_DOC
			  #[derive_impl(pallet_contracts::config_preludes::TestDefaultConfig)]
			  impl pallet_contracts::Config for Runtime{
				type Currency = Balances;
				type Schedule = Schedule;
				type CallStack = [pallet_contracts::Frame<Self>; 5];
			  }
			},
		}
	}
}

impl FromStr for CommonPallets {
	type Err = String;

	fn from_str(input: &str) -> Result<Self, Self::Err> {
		match input.to_lowercase().as_str() {
			"balances" => Ok(CommonPallets::Balances),
			"contracts" => Ok(CommonPallets::Contracts),
			_ => Err(format!("'{}' is not a valid pallet.", input)),
		}
	}
}

#[derive(Debug, Clone)]
pub(crate) struct InputPallet {
	pub(crate) pallet: CommonPallets,
	pub(crate) version: String,
}

impl FromStr for InputPallet {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let parts: Vec<&str> = s.split('=').collect();
		if parts.len() != 2 {
			return Err(format!("Invalid format: expected <pallet>=<version>, got '{}'", s));
		}

		let pallet = parts[0]
			.parse::<CommonPallets>()
			.map_err(|_| format!("Invalid pallet: '{}'.", parts[0]))?;

		// Not interested in using the Version type at all, just need to know if this &str can be
		// parsed as Version
		let _ = parts[1]
			.parse::<Version>()
			.map_err(|e| format!("Invalid version '{}': {}", parts[1], e))?;

		Ok(InputPallet { pallet, version: parts[1].to_owned() })
	}
}

#[derive(Clone)]
pub(crate) struct InputPalletParser;

impl TypedValueParser for InputPalletParser {
	type Value = InputPallet;

	fn parse_ref(
		&self,
		_cmd: &clap::Command,
		_arg: Option<&clap::Arg>,
		value: &OsStr,
	) -> Result<Self::Value, clap::Error> {
		let s = value.to_string_lossy();
		s.parse::<InputPallet>()
			.map_err(|err_msg| clap::Error::raw(clap::error::ErrorKind::InvalidValue, err_msg))
	}

	fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
		let iter = CommonPallets::value_variants()
			.iter()
			.map(|variant| variant.to_possible_value().expect("value should be possible"));
		Some(Box::new(iter))
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	#[test]
	fn get_crate_name_works() {
		assert_eq!(CommonPallets::Balances.get_crate_name(), "pallet-balances");
		assert_eq!(CommonPallets::Contracts.get_crate_name(), "pallet-contracts");
	}

	#[test]
	fn get_pallet_declaration_construct_runtime_works() {
		assert!(rustilities::parsing::syntactic_token_stream_compare(
			CommonPallets::Balances.get_pallet_declaration_construct_runtime(),
			parse_quote! { Balances: pallet_balances, }
		));

		assert!(rustilities::parsing::syntactic_token_stream_compare(
			CommonPallets::Contracts.get_pallet_declaration_construct_runtime(),
			parse_quote! { Contracts: pallet_contracts, }
		));
	}

	#[test]
	fn get_pallet_declaration_runtime_module_works() {
		assert_eq!(
			CommonPallets::Balances.get_pallet_declaration_runtime_module(parse_quote!(1)),
			parse_quote! {
				///TEMP_DOC
				#[runtime::pallet_index(1)]
				pub type Balances = pallet_balances;
			}
		);
		assert_eq!(
			CommonPallets::Contracts.get_pallet_declaration_runtime_module(parse_quote!(1)),
			parse_quote! {
				///TEMP_DOC
				#[runtime::pallet_index(1)]
				pub type Contracts = pallet_contracts;
			}
		);
	}

	#[test]
	fn get_impl_needed_use_statements_works() {
		assert_eq!(
			CommonPallets::Balances.get_impl_needed_use_statements(),
			vec![
				parse_quote! {
					///TEMP_DOC
					use crate::{System, Runtime, RuntimeEvent, RuntimeHoldReason, RuntimeCall};
				},
				parse_quote!(
					use frame_support::{parameter_types, derive_impl};
				)
			]
		);
		assert_eq!(
			CommonPallets::Contracts.get_impl_needed_use_statements(),
			vec![
				parse_quote! {
					///TEMP_DOC
					use crate::{System, Runtime, Balances. RuntimeEvent, RuntimeHoldReason, RuntimeCall};
				},
				parse_quote!(
					use frame_support::{parameter_types, derive_impl};
				)
			]
		);
	}

	#[test]
	fn get_needed_parameter_types_works() {
		assert_eq!(
			CommonPallets::Balances.get_needed_parameter_types(),
			Item::Verbatim(TokenStream::new())
		);

		assert_eq!(
			CommonPallets::Contracts.get_needed_parameter_types(),
			parse_quote! {
				///TEMP_DOC
				parameter_types!{
				  pub Schedule: pallet_contracts::Schedule<Runtime> = <pallet_contracts::Schedule<Runtime>>::default();
				}
			}
		);
	}

	#[test]
	fn get_needed_impl_block_works() {
		assert_eq!(
			CommonPallets::Balances.get_needed_impl_block(),
			parse_quote! {
				///TEMP_DOC
				#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
				impl pallet_balances::Config for Runtime {
					type AccountStore = System;
				}
			}
		);

		assert_eq!(
			CommonPallets::Contracts.get_needed_impl_block(),
			parse_quote! {
				///TEMP_DOC
			  #[derive_impl(pallet_contracts::config_preludes::TestDefaultConfig)]
			  impl pallet_contracts::Config for Runtime{
				type Currency = Balances;
				type Schedule = Schedule;
				type CallStack = [pallet_contracts::Frame<Self>; 5];
				}
			}
		);
	}

	#[test]
	fn common_pallets_from_str_works() {
		assert_eq!("balances".parse::<CommonPallets>().unwrap(), CommonPallets::Balances);
		assert_eq!("contracts".parse::<CommonPallets>().unwrap(), CommonPallets::Contracts);
		assert!("invalid".parse::<CommonPallets>().is_err());
	}

	#[test]
	fn input_pallet_from_str_valid_works() {
		let input: InputPallet = "balances=1.0.0".parse().unwrap();
		assert_eq!(input.pallet, CommonPallets::Balances);
		assert_eq!(input.version, "1.0.0");
	}

	#[test]
	fn input_pallet_from_str_invalid_format_fails() {
		assert!("balances-1.0.0".parse::<InputPallet>().is_err());
	}

	#[test]
	fn input_pallet_from_str_invalid_pallet_fails() {
		assert!("notapallet=1.0.0".parse::<InputPallet>().is_err());
	}

	#[test]
	fn input_pallet_from_str_invalid_version_fails() {
		assert!("balances=invalid".parse::<InputPallet>().is_err());
	}

	#[test]
	fn input_pallet_from_str_missing_parts_fails() {
		assert!("balances=".parse::<InputPallet>().is_err());
		assert!("=1.0.0".parse::<InputPallet>().is_err());
		assert!("balances".parse::<InputPallet>().is_err());
		assert!("=".parse::<InputPallet>().is_err());
	}

	#[test]
	fn input_pallet_parser_parse_ref_works() {
		let cmd = clap::Command::new("testcmd");
		let arg = clap::Arg::new("pallet");
		let parsed = InputPalletParser
			.parse_ref(&cmd, Some(&arg), OsStr::new("contracts=2.0.0"))
			.unwrap();
		assert_eq!(parsed.pallet, CommonPallets::Contracts);
		assert_eq!(parsed.version, "2.0.0");
	}

	#[test]
	fn input_pallet_parser_possible_values_works() {
		let possible: Vec<String> = InputPalletParser
			.possible_values()
			.unwrap()
			.map(|pv| pv.get_name().to_owned())
			.collect();
		assert!(possible.contains(&"balances".to_owned()));
		assert!(possible.contains(&"contracts".to_owned()));
	}
}
