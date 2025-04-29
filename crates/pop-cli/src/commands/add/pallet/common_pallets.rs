// SPDX-License-Identifier: GPL-3.0
use clap::ValueEnum;
use proc_macro2::{Literal, TokenStream};
use strum_macros::{EnumIter, EnumMessage};
use syn::{parse_quote, Item};

#[derive(Debug, Copy, Clone, PartialEq, EnumIter, EnumMessage, ValueEnum, Eq)]
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
					use crate::{Runtime, Balances, RuntimeEvent, RuntimeHoldReason, RuntimeCall};
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
					use crate::{Runtime, Balances, RuntimeEvent, RuntimeHoldReason, RuntimeCall};
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
}
