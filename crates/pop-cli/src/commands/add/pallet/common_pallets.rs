// SPDX-License-Identifier: GPL-3.0
use clap::ValueEnum;
use proc_macro2::{Literal, TokenStream};
use strum_macros::{EnumIter, EnumMessage};
use syn::{parse_quote, Item};

#[derive(Debug, Copy, Clone, PartialEq, EnumIter, EnumMessage, ValueEnum, Eq)]
pub(crate) enum CommonPallets {
	/// A simple, secure module for dealing with fungible assets.
	#[strum(
		message = "assets",
		detailed_message = "A simple, secure module for dealing with fungible assets.."
	)]
	Assets,
	/// The Contracts module provides functionality for the runtime to deploy and execute
	/// WebAssembly smart-contracts.
	#[strum(
		message = "contracts",
		detailed_message = "The Contracts module provides functionality for the runtime to deploy and execute WebAssembly smart-contracts."
	)]
	Contracts,
	/// Experimental module that provides functionality for the runtime to deploy and execute
	/// PolkaVM smart-contracts.
	#[strum(
		message = "revive",
		detailed_message = "Experimental module that provides functionality for the runtime to deploy and execute PolkaVM smart-contracts."
	)]
	Revive,
	/// A stateless module with helpers for dispatch management which does no re-authentication.
	#[strum(
		message = "utility",
		detailed_message = "A stateless module with helpers for dispatch management which does no re-authentication."
	)]
	Utility,
}

impl CommonPallets {
	pub(crate) fn get_crate_name(&self) -> String {
		match self {
			CommonPallets::Assets => "pallet-assets".to_owned(),
			CommonPallets::Contracts => "pallet-contracts".to_owned(),
			CommonPallets::Revive => "pallet-revive".to_owned(),
			CommonPallets::Utility => "pallet-utility".to_owned(),
		}
	}

	pub(crate) fn get_pallet_declaration_construct_runtime(&self) -> TokenStream {
		match self {
			CommonPallets::Assets => parse_quote! { Assets: pallet_assets, },
			CommonPallets::Contracts => parse_quote! { Contracts: pallet_contracts, },
			CommonPallets::Revive => parse_quote! { Revive: pallet_revive, },
			CommonPallets::Utility => parse_quote! { Utility: pallet_utility, },
		}
	}

	pub(crate) fn get_pallet_declaration_runtime_module(&self, highest_index: Literal) -> Item {
		match self {
			CommonPallets::Assets => parse_quote! {
			  ///TEMP_DOC
				#[runtime::pallet_index(#highest_index)]
				pub type Assets = pallet_assets;
			},
			CommonPallets::Contracts => parse_quote! {
			  ///TEMP_DOC
				#[runtime::pallet_index(#highest_index)]
				pub type Contracts = pallet_contracts;
			},
			CommonPallets::Revive => parse_quote! {
			  ///TEMP_DOC
				#[runtime::pallet_index(#highest_index)]
				pub type Revive = pallet_revive;
			},
			CommonPallets::Utility => parse_quote! {
				///TEMP_DOC
				#[runtime::pallet_index(#highest_index)]
				pub type Utility = pallet_utility;
			},
		}
	}

	pub(crate) fn get_impl_needed_use_statements(&self) -> Vec<Item> {
		match self {
			CommonPallets::Assets => vec![
				parse_quote!(
					///TEMP_DOC
					use crate::{
						AccountId, Balances, Runtime, RuntimeEvent, RuntimeHoldReason, RuntimeCall,
					};
				),
				parse_quote!(
					use frame_support::{parameter_types, derive_impl, traits::AsEnsureOriginWithArg};
				),
				parse_quote!(
					use frame_system::{EnsureRoot, EnsureSigned};
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
			CommonPallets::Revive => vec![
				parse_quote!(
					///TEMP_DOC
					use crate::{Runtime, Balances, RuntimeEvent, RuntimeHoldReason, RuntimeCall};
				),
				parse_quote!(
					use frame_support::{parameter_types, derive_impl};
				),
			],
			CommonPallets::Utility => vec![
				parse_quote!(
					///TEMP_DOC
					use crate::{OriginCaller, RuntimeCall, RuntimeEvent};
				),
			],
		}
	}

	pub(crate) fn get_needed_parameter_types(&self) -> Item {
		match self {
			CommonPallets::Assets => Item::Verbatim(TokenStream::new()),
			CommonPallets::Contracts => parse_quote! {
			  ///TEMP_DOC
			  parameter_types!{
				  pub Schedule: pallet_contracts::Schedule<Runtime> = <pallet_contracts::Schedule<Runtime>>::default();
			  }
			},
			CommonPallets::Revive => Item::Verbatim(TokenStream::new()),
			CommonPallets::Utility => Item::Verbatim(TokenStream::new()),
		}
	}

	pub(crate) fn get_needed_impl_block(&self) -> Item {
		match self {
			CommonPallets::Assets => parse_quote! {
			  ///TEMP_DOC
			  #[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
			  impl pallet_assets::Config for Runtime{
				type Currency = Balances;
				type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
				type ForceOrigin = EnsureRoot<AccountId>;
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
			CommonPallets::Revive => parse_quote! {
				///TEMP_DOC
				#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
				impl pallet_revive::Config for Runtime{
				  type Currency = Balances;
				  type AddressMapper = pallet_revive::AccountId32Mapper<Self>;
				}
			},
			CommonPallets::Utility => parse_quote! {
				///TEMP_DOC
				impl pallet_utility::Config for Runtime{
					type PalletsOrigin = OriginCaller;
					type RuntimeCall = RuntimeCall;
					type RuntimeEvent = RuntimeEvent;
					type WeightInfo = pallet_utility::weights::SubstrateWeight<Runtime>;
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
		assert_eq!(CommonPallets::Assets.get_crate_name(), "pallet-assets");
		assert_eq!(CommonPallets::Contracts.get_crate_name(), "pallet-contracts");
		assert_eq!(CommonPallets::Revive.get_crate_name(), "pallet-revive");
		assert_eq!(CommonPallets::Utility.get_crate_name(), "pallet-utility");
	}

	#[test]
	fn get_pallet_declaration_construct_runtime_works() {
		assert!(rustilities::parsing::syntactic_token_stream_compare(
			CommonPallets::Assets.get_pallet_declaration_construct_runtime(),
			parse_quote! { Assets: pallet_assets, }
		));
		assert!(rustilities::parsing::syntactic_token_stream_compare(
			CommonPallets::Contracts.get_pallet_declaration_construct_runtime(),
			parse_quote! { Contracts: pallet_contracts, }
		));
		assert!(rustilities::parsing::syntactic_token_stream_compare(
			CommonPallets::Revive.get_pallet_declaration_construct_runtime(),
			parse_quote! { Revive: pallet_revive, }
		));
		assert!(rustilities::parsing::syntactic_token_stream_compare(
			CommonPallets::Utility.get_pallet_declaration_construct_runtime(),
			parse_quote! { Utility: pallet_utility, }
		));
	}

	#[test]
	fn get_pallet_declaration_runtime_module_works() {
		assert_eq!(
			CommonPallets::Assets.get_pallet_declaration_runtime_module(parse_quote!(1)),
			parse_quote! {
				///TEMP_DOC
				#[runtime::pallet_index(1)]
				pub type Assets = pallet_assets;
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
		assert_eq!(
			CommonPallets::Revive.get_pallet_declaration_runtime_module(parse_quote!(1)),
			parse_quote! {
				///TEMP_DOC
				#[runtime::pallet_index(1)]
				pub type Revive = pallet_revive;
			}
		);
		assert_eq!(
			CommonPallets::Utility.get_pallet_declaration_runtime_module(parse_quote!(1)),
			parse_quote! {
				///TEMP_DOC
				#[runtime::pallet_index(1)]
				pub type Utility = pallet_utility;
			}
		);
	}

	#[test]
	fn get_impl_needed_use_statements_works() {
		assert_eq!(
			CommonPallets::Assets.get_impl_needed_use_statements(),
			vec![
				parse_quote! {
					///TEMP_DOC
					use crate::{AccountId, Balances, Runtime, RuntimeEvent, RuntimeHoldReason, RuntimeCall};
				},
				parse_quote!(
					use frame_support::{parameter_types, derive_impl, traits::AsEnsureOriginWithArg};
				),
				parse_quote!(
					use frame_system::{EnsureRoot, EnsureSigned};
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
		assert_eq!(
			CommonPallets::Revive.get_impl_needed_use_statements(),
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
		assert_eq!(
			CommonPallets::Utility.get_impl_needed_use_statements(),
			vec![
				parse_quote!(
					///TEMP_DOC
					use crate::{
						AccountId, Balances, Runtime, RuntimeEvent, RuntimeHoldReason, RuntimeCall,
						OriginCaller,
					};
				),
				parse_quote!(
					use frame_system::{EnsureRoot, EnsureSigned};
				),
			]
		);
	}

	#[test]
	fn get_needed_parameter_types_works() {
		assert_eq!(
			CommonPallets::Assets.get_needed_parameter_types(),
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
		assert_eq!(
			CommonPallets::Revive.get_needed_parameter_types(),
			Item::Verbatim(TokenStream::new())
		);
		assert_eq!(
			CommonPallets::Utility.get_needed_parameter_types(),
			Item::Verbatim(TokenStream::new())
		);
	}

	#[test]
	fn get_needed_impl_block_works() {
		assert_eq!(
			CommonPallets::Assets.get_needed_impl_block(),
			parse_quote! {
				///TEMP_DOC
				#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig)]
				impl pallet_assets::Config for Runtime {
					type Currency = Balances;
					type CreateOrigin = AsEnsureOriginWithArg<EnsureSigned<AccountId>>;
					type ForceOrigin = EnsureRoot<AccountId>;
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
		assert_eq!(
			CommonPallets::Revive.get_needed_impl_block(),
			parse_quote! {
				///TEMP_DOC
			  #[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
			  impl pallet_revive::Config for Runtime{
				type Currency = Balances;
				type AddressMapper = pallet_revive::AccountId32Mapper<Self>;
				}
			}
		);
		assert_eq!(
			CommonPallets::Utility.get_needed_impl_block(),
			parse_quote! {
				///TEMP_DOC
			  impl pallet_utility::Config for Runtime{
				type PalletsOrigin = OriginCaller;
				type RuntimeCall = RuntimeCall;
				type RuntimeEvent = RuntimeEvent;
				type WeightInfo = pallet_utility::weights::SubstrateWeight<Runtime>;
				}
			}
		);
	}
}
