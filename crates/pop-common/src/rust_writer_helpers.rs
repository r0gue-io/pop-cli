// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use fs_rollback::Rollback;
use proc_macro2::{Literal, Span};
use rust_writer::{
	ast::{
		finder::{Finder, ToFind},
		implementors::ItemToFile,
		mutator::{Mutator, ToMutate},
	},
	preserver::Preserver,
};
use std::{
	cmp,
	path::{Path, PathBuf},
};
use syn::{
	parse_quote, File, Ident, ImplItem, Item, ItemMacro, ItemMod, ItemType, Macro, Meta, MetaList,
	Path as syn_Path,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq)]
pub enum DefaultConfigType {
	Default { type_default_impl: ImplItem },
	NoDefault,
	NoDefaultBounds { type_default_impl: ImplItem },
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeUsedMacro {
	Runtime,
	ConstructRuntime,
}

// Not more than 256 pallets are included in a runtime
pub type PalletIndex = u8;

/// Find the highest implemented pallet index in the outer enum if using the macro
/// #[runtime].
pub fn find_highest_pallet_index(ast: &File) -> Result<Literal, Error> {
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
							if let Meta::List(MetaList {
								path: syn_Path { segments, .. }, ..
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
	Ok(Literal::u8_unsuffixed(highest_index.saturating_add(1)))
}

/// Determine whether a runtime's ast uses the construct_runtime! macro or the #[runtime] macro.
pub fn find_used_runtime_macro(ast: &File) -> Result<RuntimeUsedMacro, Error> {
	for item in &ast.items {
		match item {
			Item::Mod(ItemMod { ident, .. }) if *ident == "runtime" => {
				return Ok(RuntimeUsedMacro::Runtime);
			},
			Item::Macro(ItemMacro {
				mac: Macro { path: syn_Path { segments, .. }, .. }, ..
			}) if segments.iter().any(|segment| segment.ident == "construct_runtime") => {
				return Ok(RuntimeUsedMacro::ConstructRuntime);
			},
			_ => (),
		}
	}
	return Err(Error::Descriptive(format!("Unable to find a runtime declaration in runtime file")));
}

pub fn compute_pallet_related_paths(runtime_path: &Path) -> (PathBuf, PathBuf, PathBuf, PathBuf) {
	let runtime_src_path = runtime_path.join("src");
	let runtime_lib_path = runtime_src_path.join("lib.rs");
	let configs_rs_path = runtime_src_path.join("configs.rs");
	let configs_folder_path = runtime_src_path.join("configs");
	let configs_mod_path = configs_folder_path.join("mod.rs");
	(runtime_lib_path, configs_rs_path, configs_folder_path, configs_mod_path)
}

pub fn compute_new_pallet_impl_path<'a>(
	mut rollback: Rollback<'a>,
	runtime_lib_path: &'a Path,
	configs_rs_path: &'a Path,
	configs_folder_path: &'a Path,
	configs_mod_path: &'a Path,
	pallet_config_path: &'a Path,
	pallet_name: &str,
) -> Result<Rollback<'a>, Error> {
	let pallet_name_ident = Ident::new(pallet_name, Span::call_site());

	let mod_preserver = Preserver::new("mod");
	let pub_mod_preserver = Preserver::new("pub mod");

	let pallet_mod_implementor = ItemToFile { item: parse_quote!(mod #pallet_name_ident;) };

	match (configs_rs_path.is_file(), configs_mod_path.is_file()) {
		// The runtime is using a configs module without the mod.rs sintax
		(true, false) => {
			if rollback.get_noted_file(&configs_rs_path).is_none() {
				rollback.note_file(&configs_rs_path)?;
			}

			let roll_configs_rs_path = rollback
				.get_noted_file(&configs_rs_path)
				.expect("This file has been noted above; qed;");
			let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
				roll_configs_rs_path,
				&[&mod_preserver, &pub_mod_preserver],
			)?;

			let mut finder = Finder::default().to_find(&pallet_mod_implementor);
			let pallet_already_declared = finder.find(&preserved_ast);
			if !pallet_already_declared {
				let mut mutator = Mutator::default().to_mutate(&pallet_mod_implementor);
				mutator.mutate(&mut preserved_ast)?;
				rust_writer::preserver::resolve_preserved(&preserved_ast, roll_configs_rs_path)?;
			} else {
				return Err(Error::Descriptive(format!("{pallet_name} is already in use.")));
			}

			rollback.new_file(&pallet_config_path)?;
			Ok(rollback)
		},
		// The runtime is using a configs module with the mod.rs syntax
		(false, true) => {
			if rollback.get_noted_file(&configs_mod_path).is_none() {
				rollback.note_file(&configs_mod_path)?;
			}

			let roll_configs_mod_path = rollback
				.get_noted_file(&configs_mod_path)
				.expect("This file has been noted above; qed;");
			let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
				roll_configs_mod_path,
				&[&mod_preserver, &pub_mod_preserver],
			)?;

			let mut finder = Finder::default().to_find(&pallet_mod_implementor);
			let pallet_already_declared = finder.find(&preserved_ast);
			if !pallet_already_declared {
				let mut mutator = Mutator::default().to_mutate(&pallet_mod_implementor);
				mutator.mutate(&mut preserved_ast)?;
				rust_writer::preserver::resolve_preserved(&preserved_ast, roll_configs_mod_path)?;
			} else {
				return Err(Error::Descriptive(format!("{pallet_name} is already in use.")));
			}

			rollback.new_file(&pallet_config_path)?;
			Ok(rollback)
		},
		// The runtime isn't using a configs module yet, we opt for the configs.rs
		// convention
		(false, false) => {
			let configs_mod_implementor = ItemToFile {
				item: parse_quote!(
					pub mod configs;
				),
			};
			if rollback.get_noted_file(&runtime_lib_path).is_none() {
				rollback.note_file(&runtime_lib_path)?;
			}

			let roll_runtime_lib_path = rollback
				.get_noted_file(&runtime_lib_path)
				.expect("This file has been noted above; qed;");
			let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
				roll_runtime_lib_path,
				&[&mod_preserver, &pub_mod_preserver],
			)?;

			let mut finder = Finder::default().to_find(&configs_mod_implementor);
			let configs_already_declared = finder.find(&preserved_ast);
			if !configs_already_declared {
				let mut mutator = Mutator::default().to_mutate(&configs_mod_implementor);
				mutator.mutate(&mut preserved_ast)?;
				rust_writer::preserver::resolve_preserved(&preserved_ast, roll_runtime_lib_path)?;
			}

			rollback.new_file(&configs_rs_path)?;
			rollback.new_dir(&configs_folder_path)?;
			rollback.new_file(&pallet_config_path)?;

			let roll_configs_rs_path = rollback
				.get_new_file(&configs_rs_path)
				.expect("The new file has been noted above; qed");

			// New file so we can mutate it directly.
			let mut ast = rust_writer::preserver::preserve_and_parse(roll_configs_rs_path, &[])?;
			let mut mutator = Mutator::default().to_mutate(&pallet_mod_implementor);
			mutator.mutate(&mut ast)?;
			rust_writer::preserver::resolve_preserved(&ast, roll_configs_rs_path)?;

			Ok(rollback)
		},
		// Both approaches at the sime time aren't supported by the compiler, so this is
		// unreachable in a compiling project
		(true, true) => unreachable!(),
	}
}
