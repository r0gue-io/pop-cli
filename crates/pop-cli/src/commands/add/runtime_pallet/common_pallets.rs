// SPDX-License-Identifier: GPL-3.0
use clap::ValueEnum;
use proc_macro2::{Literal, TokenStream};
use strum_macros::{EnumIter, EnumMessage};
use syn::{parse_quote, Item};

#[derive(Debug, Copy, Clone, PartialEq, EnumIter, EnumMessage, ValueEnum)]
pub enum CommonPallets {
	/// Add pallet-balances to your runtime.
	#[strum(message = "Balances", detailed_message = "Add pallet-balances to your runtime.")]
	Balances,
	/// Add pallet-contracts to your runtime.
	#[strum(message = "Contracts", detailed_message = "Add pallet-contracts to your runtime.")]
	Contracts,
}

impl CommonPallets {
	pub fn get_crate_name(&self) -> String {
		match self {
			CommonPallets::Balances => "pallet-balances".to_string(),
			CommonPallets::Contracts => "pallet-contracts".to_string(),
		}
	}

	pub fn get_pallet_declaration_construct_runtime(&self) -> TokenStream {
		match self {
			CommonPallets::Balances => parse_quote! { Balances: pallet_balances, },
			CommonPallets::Contracts => parse_quote! { Contracts: pallet_contracts, },
		}
	}

	pub fn get_pallet_declaration_runtime_module(&self, highest_index: Literal) -> Item {
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

	pub fn get_version(&self) -> String {
		match self {
			CommonPallets::Balances => "39.0.0".to_string(),
			CommonPallets::Contracts => "27.0.0".to_string(),
		}
	}

	pub fn get_impl_needed_use_statements(&self) -> Vec<Item> {
		match self {
			CommonPallets::Balances => vec![parse_quote!(
				use crate::System;
			)],
			CommonPallets::Contracts => vec![parse_quote!(
				use crate::Balances;
			)],
		}
	}

	pub fn get_needed_parameter_types(&self) -> Item {
		match self {
			CommonPallets::Balances => parse_quote!(),
			CommonPallets::Contracts => parse_quote! {
			parameter_types!{
				pub Schedule: pallet_contracts::Schedule<Runtime> = Default::default();
			}
				},
		}
	}

	pub fn get_needed_impl_block(&self) -> Item {
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
				type Schedule = [pallet_contracts::Frame<Self>; 5];
				type CallStack = Schedule;
			  }
			},
		}
	}
}
