// SPDX-License-Identifier: GPL-3.0
use clap::ValueEnum;
use pop_common::rust_writer_helpers::DefaultConfigType;
use strum_macros::{EnumIter, EnumMessage};
use syn::{parse_quote, ImplItem, Item, TraitItem};

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
	pub fn get_type_definition(&self, default_config: DefaultConfigType) -> TraitItem {
		match (self, default_config) {
			(CommonTypes::RuntimeEvent, DefaultConfigType::NoDefaultBounds { .. }) =>
				parse_quote! {
				  /// The aggregated event type of the runtime.
				#[pallet::no_default_bounds]
				  type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
				},
			(CommonTypes::RuntimeEvent, DefaultConfigType::Default { .. }) => parse_quote! {
			/// The aggregated event type of the runtime.
			  type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
			},
			(CommonTypes::RuntimeOrigin, DefaultConfigType::NoDefaultBounds { .. }) =>
				parse_quote! {
				/// The aggregated origin type of the runtime.
					#[pallet::no_default_bounds]
					type RuntimeOrigin: From<OriginFor<Self>>;
				  },
			(CommonTypes::RuntimeOrigin, DefaultConfigType::Default { .. }) => parse_quote! {
			/// The aggregated origin type of the runtime.
			  type RuntimeOrigin: From<OriginFor<Self>>;
			},
			(CommonTypes::RuntimeHoldReason, DefaultConfigType::NoDefaultBounds { .. }) =>
				parse_quote! {
				/// A reason for placing a hold on funds
					#[pallet::no_default_bounds]
					type RuntimeHoldReason: From<HoldReason>;
				  },
			(CommonTypes::RuntimeHoldReason, DefaultConfigType::Default { .. }) => parse_quote! {
			/// A reason for placing a hold on funds
			  type RuntimeHoldReason: From<HoldReason>;
			},
			(CommonTypes::RuntimeFreezeReason, DefaultConfigType::NoDefaultBounds { .. }) =>
				parse_quote! {
				/// A reason for placing a freeze on funds
					#[pallet::no_default_bounds]
					type RuntimeFreezeReason: VariantCount;
				  },
			(CommonTypes::RuntimeFreezeReason, DefaultConfigType::Default { .. }) => parse_quote! {
			/// A reason for placing a freeze on funds
			  type RuntimeFreezeReason: VariantCount;
			},
			(CommonTypes::Fungibles, DefaultConfigType::NoDefault) => parse_quote! {
			  #[pallet::no_default]
			  type Fungibles:
				fungible::Inspect<Self::AccountId> +
				fungible::Mutate<Self::AccountId> +
				fungible::hold::Inspect<Self::AccountId> +
				fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason> +
				fungible::freeze::Inspect<Self::AccountId> +
				fungible::freeze::Mutate<Self::AccountId>;
			},
			(CommonTypes::Fungibles, DefaultConfigType::Default { .. }) => parse_quote! {
			  type Fungibles:
				fungible::Inspect<Self::AccountId> +
				fungible::Mutate<Self::AccountId> +
				fungible::hold::Inspect<Self::AccountId> +
				fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason> +
				fungible::freeze::Inspect<Self::AccountId> +
				fungible::freeze::Mutate<Self::AccountId>;
			},
			// By construction this case shouldn't occur.
			_ => parse_quote! {},
		}
	}

	pub fn get_common_runtime_value(&self) -> ImplItem {
		match self {
			CommonTypes::RuntimeEvent => parse_quote! {type RuntimeEvent = RuntimeEvent;},
			CommonTypes::RuntimeOrigin => parse_quote! {type RuntimeOrigin = RuntimeOrigin;},
			CommonTypes::RuntimeHoldReason =>
				parse_quote! {type RuntimeHoldReason = RuntimeHoldReason;},
			CommonTypes::RuntimeFreezeReason =>
				parse_quote! {type RuntimeFreezeReason = RuntimeFreezeReason;},
			CommonTypes::Fungibles => parse_quote! {type Fungibles = Balances;},
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

	pub fn get_needed_use_statements(&self) -> Vec<Item> {
		match self {
			CommonTypes::RuntimeEvent => Vec::new(),
			CommonTypes::RuntimeOrigin => Vec::new(),
			CommonTypes::RuntimeHoldReason => Vec::new(),
			CommonTypes::RuntimeFreezeReason =>
				vec![parse_quote! {use frame::traits::VariantCount;}],
			CommonTypes::Fungibles => vec![parse_quote! {use frame::traits::fungible;}],
		}
	}

	pub fn get_needed_composite_enums(&self) -> Vec<Item> {
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
