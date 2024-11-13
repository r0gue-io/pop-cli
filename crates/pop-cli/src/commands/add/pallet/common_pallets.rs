// SPDX-License-Identifier: GPL-3.0
use clap::ValueEnum;
use pop_common::rust_writer::types::ParameterTypes;
use proc_macro2::Span;
use strum_macros::{EnumIter, EnumMessage};
use syn::{parse_quote, Ident, Type};

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

	pub fn get_version(&self) -> String {
		match self {
			CommonPallets::Balances => "39.0.0".to_string(),
			CommonPallets::Contracts => "27.0.0".to_string(),
		}
	}

	pub fn get_parameter_types(&self) -> Vec<ParameterTypes> {
		match self {
			CommonPallets::Balances => Vec::new(),
			CommonPallets::Contracts => vec![ParameterTypes {
				ident: Ident::new("Schedule", Span::call_site()),
				type_: parse_quote! {pallet_contracts::Schedule<Runtime>},
				value: parse_quote! {Default::default()},
			}],
		}
	}

	pub fn get_config_types(&self) -> Vec<Ident> {
		match self {
			CommonPallets::Balances => vec![Ident::new("AccountStore", Span::call_site())],
			CommonPallets::Contracts => vec![
				Ident::new("Currency", Span::call_site()),
				Ident::new("Schedule", Span::call_site()),
				Ident::new("CallStack", Span::call_site()),
			],
		}
	}

	pub fn get_config_values(&self) -> Vec<Type> {
		match self {
			CommonPallets::Balances => {
				vec![parse_quote! {System}]
			},
			CommonPallets::Contracts => {
				vec![
					parse_quote! {Balances},
					parse_quote! {[pallet_contracts::Frame<Self>; 5]},
					parse_quote! {Schedule},
				]
			},
		}
	}

	pub fn get_default_config(&self) -> bool {
		match self {
			CommonPallets::Balances => true,
			CommonPallets::Contracts => true,
		}
	}
}
