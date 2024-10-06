// SPDX-License-Identifier: GPL-3.0
use clap::ValueEnum;
use pop_common::rust_writer::types::DefaultConfigType;
use strum_macros::{EnumIter, EnumMessage};
use syn::{parse_quote, ImplItem, ItemEnum, ItemUse, TraitBound, Type};

/// This enum is used to register from the CLI which types that are kind of usual in config traits
/// are included in the pallet
#[derive(Debug, Copy, Clone, PartialEq, EnumIter, EnumMessage, ValueEnum)]
pub enum CommonTypes {
	/// This type will enable your pallet to emit events.
	#[strum(
		message = "RuntimeEvent",
		detailed_message = "This type will enable your pallet to emit events."
	)]
	RuntimeEvent,
	/// This type will be helpful if your pallet needs to deal with the outer RuntimeOrigin enum,
	/// or if your pallet needs to use custom origins.
	#[strum(
		message = "RuntimeOrigin",
		detailed_message = "This type will be helpful if your pallet needs to deal with the outer RuntimeOrigin enum, or if your pallet needs to use custom origins."
	)]
	RuntimeOrigin,
	/// This type will be helpful if your pallet needs to hold funds.
	#[strum(
		message = "RuntimeHoldReason",
		detailed_message = "This type will be helpful if your pallet needs to hold funds."
	)]
	RuntimeHoldReason,
	/// This type will be helpful if your pallet needs to freeze funds.
	#[strum(
		message = "RuntimeFreezeReason",
		detailed_message = "This type will be helpful if your pallet needs to freeze funds."
	)]
	RuntimeFreezeReason,
	/// This type will allow your pallet to manage fungible assets. If you add this type to your
	/// pallet, RuntimeHoldReason and RuntimeFreezeReason will be added as well
	#[strum(
		message = "Fungibles",
		detailed_message = "This type will allow your pallet to manage fungible assets. If you add this type to your pallet, RuntimeHoldReason and RuntimeFreezeReason will be added as well"
	)]
	Fungibles,
}

impl CommonTypes {
	pub fn get_common_trait_bounds(&self) -> Vec<TraitBound> {
		match self {
			CommonTypes::RuntimeEvent => vec![
				parse_quote! {From<Event<Self>>},
				parse_quote! {IsType<<Self as frame_system::Config>::RuntimeEvent>},
			],
			CommonTypes::RuntimeOrigin => vec![parse_quote! {From<OriginFor<Self>>}],
			CommonTypes::RuntimeHoldReason => vec![parse_quote! {From<HoldReason>}],
			CommonTypes::RuntimeFreezeReason => vec![parse_quote! {VariantCount}],
			CommonTypes::Fungibles => vec![
				parse_quote! {fungible::Inspect<Self::AccountId>},
				parse_quote! {fungible::Mutate<Self::AccountId>},
				parse_quote! {fungible::hold::Inspect<Self::AccountId>},
				parse_quote! {fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason>},
				parse_quote! {fungible::freeze::Inspect<Self::AccountId>},
				parse_quote! {fungible::freeze::Mutate<Self::AccountId>},
			],
		}
	}

	pub fn get_common_runtime_value(&self) -> Type {
		match self {
			CommonTypes::RuntimeEvent => parse_quote! {RuntimeEvent},
			CommonTypes::RuntimeOrigin => parse_quote! {RuntimeOrigin},
			CommonTypes::RuntimeHoldReason => parse_quote! {RuntimeHoldReason},
			CommonTypes::RuntimeFreezeReason => parse_quote! {RuntimeFreezeReason},
			CommonTypes::Fungibles => parse_quote! {Balances},
		}
	}

	pub fn get_default_config(&self) -> DefaultConfigType {
		match self {
			CommonTypes::RuntimeEvent => DefaultConfigType::NoDefaultBounds {
				type_default_impl: ImplItem::Type(parse_quote! {
					#[inject_runtime_type]
					type RuntimeEvent = ();
				}),
			},
			CommonTypes::RuntimeOrigin => DefaultConfigType::NoDefaultBounds {
				type_default_impl: ImplItem::Type(parse_quote! {
					#[inject_runtime_type]
					type RuntimeOrigin = ();
				}),
			},
			CommonTypes::RuntimeHoldReason => DefaultConfigType::NoDefaultBounds {
				type_default_impl: ImplItem::Type(parse_quote! {
					#[inject_runtime_type]
					type RuntimeHoldReason = ();
				}),
			},
			CommonTypes::RuntimeFreezeReason => DefaultConfigType::NoDefaultBounds {
				type_default_impl: ImplItem::Type(parse_quote! {
					#[inject_runtime_type]
					type RuntimeFreezeReason = ();
				}),
			},
			CommonTypes::Fungibles => DefaultConfigType::NoDefault,
		}
	}

	pub fn get_needed_use_statements(&self) -> Vec<ItemUse> {
		match self {
			CommonTypes::RuntimeEvent => Vec::new(),
			CommonTypes::RuntimeOrigin => Vec::new(),
			CommonTypes::RuntimeHoldReason => Vec::new(),
			CommonTypes::RuntimeFreezeReason =>
				vec![parse_quote! {use frame::traits::VariantCount;}],
			CommonTypes::Fungibles => vec![parse_quote! {use frame::traits::fungible;}],
		}
	}

	pub fn get_needed_composite_enums(&self) -> Vec<ItemEnum> {
		match self {
			CommonTypes::RuntimeEvent => Vec::new(),
			CommonTypes::RuntimeOrigin => Vec::new(),
			CommonTypes::RuntimeHoldReason => vec![parse_quote! {
				/// A reason for the pallet placing a hold on funds.
				#[pallet::composite_enum]
				pub enum HoldReason {
					/// Some hold reason
					#[codec(index = 0)]
					SomeHoldReason,
				}
			}],
			CommonTypes::RuntimeFreezeReason => Vec::new(),
			CommonTypes::Fungibles => Vec::new(),
		}
	}
}

pub fn complete_dependencies(mut types: Vec<CommonTypes>) -> Vec<CommonTypes> {
	if types.contains(&CommonTypes::Fungibles) {
		if !types.contains(&CommonTypes::RuntimeHoldReason) {
			types.push(CommonTypes::RuntimeHoldReason);
		}

		if !types.contains(&CommonTypes::RuntimeFreezeReason) {
			types.push(CommonTypes::RuntimeFreezeReason);
		}
	}
	types
}
