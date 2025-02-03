// SPDX-License-Identifier: GPL-3.0

use crate::{rust_writer::types::RuntimeUsedMacro, Error};
use std::cmp;
use syn::{
	File, Item, ItemEnum, ItemMacro, ItemMod, ItemType, ItemUse, Macro, Meta, MetaList, Path,
};

#[cfg(test)]
mod tests;

// Not more than 256 pallets are included in a runtime
type PalletIndex = u8;

/// Find the highest implemented pallet index in the outer enum if using the macro
/// #[runtime].
pub(crate) fn find_highest_pallet_index(ast: &File) -> Result<PalletIndex, Error> {
	let mut highest_index: PalletIndex = 0;
	let mut found = false;
	for item in &ast.items {
		match item {
			Item::Mod(ItemMod { ident, content, .. })
				if *ident == "runtime" && content.is_some() =>
			{
				let (_, items) =
					content.as_ref().expect("content is always Some thanks to the match guard");
				for item in items {
					if let Item::Type(ItemType { attrs, .. }) = item {
						if let Some(pallet_index_attribute) = attrs.iter().find(|attribute| {
							if let Meta::List(MetaList { path: Path { segments, .. }, .. }) =
								&attribute.meta
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
								pallet_index = tokens
                                    .clone()
                                    .into_iter()
                                    .next()
                                    .expect("This iterator has one element due to the attribute shape; qed;")
                                    .to_string()
                                    .parse::<PalletIndex>()
                                    .expect("The macro #[runtime::pallet_index(n)] is only valid if n is a valid number, so we can parse it to PalletIndex; qed;");
							}
							// Despite the pallets will likely be ordered by call_index in the
							// runtime, that's not necessarily true, so we keep the highest index in
							// order to give the added pallet the next index
							highest_index = cmp::max(highest_index, pallet_index);
							found = true;
						}
					}
				}
			},
			_ => continue,
		}
	}

	if !found {
		return Err(Error::Descriptive(
			format! {"Unable to find the highest pallet index in runtime file"},
		))
	}
	Ok(highest_index)
}

/// Determine whether a runtime's ast uses the construct_runtime! macro or the #[runtime] macro.
pub(crate) fn find_used_runtime_macro(ast: &File) -> Result<RuntimeUsedMacro, Error> {
	for item in &ast.items {
		match item {
			Item::Mod(ItemMod { ident, .. }) if *ident == "runtime" => {
				return Ok(RuntimeUsedMacro::Runtime);
			},
			Item::Macro(ItemMacro { mac: Macro { path: Path { segments, .. }, .. }, .. })
				if segments.iter().any(|segment| segment.ident == "construct_runtime") =>
			{
				return Ok(RuntimeUsedMacro::ConstructRuntime);
			},
			_ => (),
		}
	}
	return Err(Error::Descriptive(format!("Unable to find a runtime declaration in runtime file")));
}

pub(crate) fn find_use_statement(ast: &File, use_statement: &ItemUse) -> bool {
	for item in &ast.items {
		match item {
			Item::Use(item_use) if item_use == use_statement => return true,
			_ => continue,
		}
	}
	false
}

pub(crate) fn find_composite_enum(ast: &File, composite_enum: &ItemEnum) -> bool {
	for item in &ast.items {
		match item {
			Item::Mod(ItemMod { ident, content, .. })
				if *ident == "pallet" && content.is_some() =>
			{
				let (_, items) =
					content.as_ref().expect("content is always Some thanks to the match guard");
				for item in items {
					match item {
						Item::Enum(ItemEnum { ident, attrs, .. })
							if *ident == composite_enum.ident &&
								attrs.iter().any(|attribute| {
									if let Meta::Path(Path { segments, .. }) = &attribute.meta {
										// It's enough checking than composite_enum is in the path
										segments
											.iter()
											.any(|segment| segment.ident == "composite_enum")
									} else {
										false
									}
								}) =>
							return true,
						_ => continue,
					}
				}
			},
			_ => continue,
		}
	}
	false
}
