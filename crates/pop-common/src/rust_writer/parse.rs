// SPDX-License-Identifier: GPL-3.0

use crate::rust_writer::types::RuntimeUsedMacro;
use std::cmp;
use syn::{File, Item, ItemMacro, ItemMod, ItemType, Macro, Meta, MetaList, ItemUse, ItemEnum};

/// Find the highest implemented pallet index in the outer enum if using the macro
/// #[runtime]. We suppose it's a u8, it's not likely that a runtime implements more than 256
/// pallets. Also determine if the runtime uses either #[runtime] or construct_runtime!, in
/// the latter we don't find the highest_index as specifying indexes isn't mandatory and
/// construct runtime will infer the pallet index.
pub(crate) fn find_highest_pallet_index_and_runtime_macro_version(
	ast: &File,
) -> (u8, RuntimeUsedMacro) {
	let mut highest_index = 0u8;
	let mut used_macro = RuntimeUsedMacro::NotFound;
	for item in &ast.items {
		match item {
			// If runtime is using the new macro #[runtime], the pallets are listed inside a
			// module called runtime as types annotated with #[runtime::pallet_index(n)] where n
			// is the pallet index.
			Item::Mod(ItemMod { ident, content, .. })
				if *ident == "runtime" && content.is_some() =>
			{
				used_macro = RuntimeUsedMacro::Runtime;
				let (_, items) =
					content.as_ref().expect("content is always Some thanks to the match guard");
				for item in items {
					if let Item::Type(ItemType { attrs, .. }) = item {
						if let Some(pallet_index_attribute) = attrs.iter().find(|attribute| {
							if let Meta::List(MetaList {
								path: syn::Path { segments, .. }, ..
							}) = &attribute.meta
							{
								segments.iter().any(|segment| segment.ident == "pallet_index")
							} else {
								false
							}
						}) {
							// As the attribute at this point is for sure
							// #[runtime::pallet_index(n)], so meta is a MetaList where tokens
							// is a TokenStream of exactly one element: the literal n.
							let mut pallet_index = 0u8;
							if let Meta::List(MetaList { tokens, .. }) =
								&pallet_index_attribute.meta
							{
								pallet_index = tokens.clone().into_iter().next().expect("This iterator has one element due to the attribute shape; qed;").to_string().parse::<u8>().expect("The macro #[runtime::pallet_index(n)] is only valid if n is a valid number, so we can parse it to u8; qed;");
							}
							// Despite the pallets will likely be ordered by call_index in the
							// runtime, that's not always true, so we keep the highest index in
							// order to give the added pallet the next index
							highest_index = cmp::max(highest_index, pallet_index);
						}
					}
				}
			},
			// If runtime is using the construct_runtime! macro, keep track of it
			Item::Macro(ItemMacro {
				mac: Macro { path: syn::Path { segments, .. }, .. }, ..
			}) if segments.iter().any(|segment| segment.ident == "construct_runtime") =>
				used_macro = RuntimeUsedMacro::ConstructRuntime,
			_ => continue,
		}
	}
	(highest_index, used_macro)
}

pub(crate) fn find_use_statement(ast: &File, use_statement: &ItemUse) -> bool{
    for item in &ast.items{
        match item{
            Item::Use(item_use) if item_use == use_statement => return true,
            _ => continue
        }
    }
    false
}

pub(crate) fn find_composite_enum(ast: &File, composite_enum: &ItemEnum) -> bool{
    for item in &ast.items{
        match item{
			Item::Mod(ItemMod { ident, content, .. })
				if *ident == "pallet" && content.is_some() =>
			{
				let (_, items) =
					content.as_ref().expect("content is always Some thanks to the match guard");
				for item in items {
                    match item{
                        Item::Enum(ItemEnum{ident,..}) if *ident == composite_enum.ident => return true,
                        _ => continue
                    }
                }
            },
            _ => continue
        }
    }
    false
}
