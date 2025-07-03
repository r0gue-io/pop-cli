// SPDX-License-Identifier: GPL-3.0

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
	parse_quote, File, Ident, Item, ItemMacro, ItemMod, ItemType, Macro, Meta, MetaList,
	Path as syn_Path,
};

// The different ways available to construct a runtime
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RuntimeUsedMacro {
	// The macro #[runtime]
	Runtime,
	// The macro construct_runtime!
	ConstructRuntime,
}

#[derive(Debug, Clone)]
pub(crate) struct PalletConfigRelatedPaths {
	pub(crate) runtime_lib_path: PathBuf,
	pub(crate) configs_rs_path: PathBuf,
	pub(crate) configs_folder_path: PathBuf,
	pub(crate) configs_mod_path: PathBuf,
}

// Not more than 256 pallets are included in a runtime
pub(crate) type PalletIndex = u8;

// Find the highest implemented pallet index in the outer enum if using the macro
// #[runtime].
pub(crate) fn find_highest_pallet_index(ast: &File) -> anyhow::Result<Literal> {
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
							// #[runtime::pallet_index(n)], meta is a MetaList where tokens
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
		return Err(anyhow::anyhow!(
			format! {"Unable to find the highest pallet index in runtime file"},
		));
	}
	Ok(Literal::u8_unsuffixed(highest_index.saturating_add(1)))
}

// Determine whether a runtime's ast uses the construct_runtime! macro or the #[runtime] macro.
pub(crate) fn find_used_runtime_macro(ast: &File) -> anyhow::Result<RuntimeUsedMacro> {
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
	Err(anyhow::anyhow!(format!("Unable to find a runtime declaration in runtime file")))
}

pub(crate) fn compute_pallet_related_paths(runtime_path: &Path) -> PalletConfigRelatedPaths {
	let runtime_src_path = runtime_path.join("src");
	let runtime_lib_path = runtime_src_path.join("lib.rs");
	let configs_rs_path = runtime_src_path.join("configs.rs");
	let configs_folder_path = runtime_src_path.join("configs");
	let configs_mod_path = configs_folder_path.join("mod.rs");
	PalletConfigRelatedPaths {
		runtime_lib_path,
		configs_rs_path,
		configs_folder_path,
		configs_mod_path,
	}
}

// Creates the structure for the path to the file when the new impl block will be added and add it
// to an existing rollback.
pub(crate) fn create_new_pallet_impl_path_structure<'a>(
	mut rollback: Rollback<'a>,
	pallet_config_related_paths: &'a PalletConfigRelatedPaths,
	pallet_config_path: &'a Path,
	pallet_name: &str,
) -> anyhow::Result<Rollback<'a>> {
	let PalletConfigRelatedPaths {
		runtime_lib_path,
		configs_rs_path,
		configs_folder_path,
		configs_mod_path,
	} = pallet_config_related_paths;
	let pallet_name_ident = Ident::new(pallet_name, Span::call_site());

	let mod_preserver = Preserver::new("mod");
	let pub_mod_preserver = Preserver::new("pub mod");

	let pallet_mod_implementor = ItemToFile { item: parse_quote!(mod #pallet_name_ident;) };

	match (configs_rs_path.is_file(), configs_mod_path.is_file()) {
		// The runtime is using a configs module without the mod.rs sintax
		(true, false) => {
			if rollback.get_noted_file(configs_rs_path).is_none() {
				rollback.note_file(configs_rs_path)?;
			}

			let roll_configs_rs_path = rollback
				.get_noted_file(configs_rs_path)
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
			}

			rollback.new_file(pallet_config_path)?;
			Ok(rollback)
		},
		// The runtime is using a configs module with the mod.rs syntax
		(false, true) => {
			if rollback.get_noted_file(configs_mod_path).is_none() {
				rollback.note_file(configs_mod_path)?;
			}

			let roll_configs_mod_path = rollback
				.get_noted_file(configs_mod_path)
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
			}

			rollback.new_file(pallet_config_path)?;
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
			if rollback.get_noted_file(runtime_lib_path).is_none() {
				rollback.note_file(runtime_lib_path)?;
			}

			let roll_runtime_lib_path = rollback
				.get_noted_file(runtime_lib_path)
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

			rollback.new_file(configs_rs_path)?;
			rollback.new_dir(configs_folder_path)?;
			rollback.new_file(pallet_config_path)?;

			let roll_configs_rs_path = rollback
				.get_new_file(configs_rs_path)
				.expect("The new file has been noted above; qed");

			// New file so we can mutate it directly.
			let mut ast = rust_writer::preserver::preserve_and_parse(roll_configs_rs_path, &[])?;
			let mut mutator = Mutator::default().to_mutate(&pallet_mod_implementor);
			mutator.mutate(&mut ast)?;
			rust_writer::preserver::resolve_preserved(&ast, roll_configs_rs_path)?;

			Ok(rollback)
		},
		(true, true) => unreachable!("Both approaches not supported by the compiler; qed;"),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use pop_parachains::{Config, Parachain};
	use similar::{ChangeTag, TextDiff};
	use std::path::PathBuf;

	fn setup_template_runtime_v2_macro() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		let config = Config {
			symbol: "DOT".to_string(),
			decimals: 18,
			initial_endowment: "1000000".to_string(),
		};
		pop_parachains::instantiate_standard_template(
			&Parachain::Standard,
			temp_dir.path(),
			config,
			None,
		)?;
		Ok(temp_dir)
	}

	fn setup_template_construct_runtime_macro() -> Result<tempfile::TempDir> {
		let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
		pop_parachains::instantiate_openzeppelin_template(
			&Parachain::OpenZeppelinGeneric,
			temp_dir.path(),
			Some("v2.0.3".to_owned()),
		)?;
		Ok(temp_dir)
	}

	#[test]
	fn find_highest_pallet_index_works() {
		let temp_dir =
			setup_template_runtime_v2_macro().expect("Failed to setup template and instantiate");

		let ast = syn::parse_file(
			&std::fs::read_to_string(temp_dir.path().join("runtime").join("src").join("lib.rs"))
				.expect("File should be readable; qed;"),
		)
		.expect("File should be parseable; qed;");

		let highest_index = find_highest_pallet_index(&ast)
			.expect("find_highest_pallet_index is supposed to be Ok");

		// The highest index in the template is 33
		assert_eq!(highest_index.to_string(), "34");
	}

	#[test]
	fn find_highest_pallet_index_fails_if_input_doesnt_use_runtime_macro() {
		let temp_dir = setup_template_construct_runtime_macro()
			.expect("Failed to setup template and instantiate");

		let ast = syn::parse_file(
			&std::fs::read_to_string(temp_dir.path().join("runtime").join("src").join("lib.rs"))
				.expect("File should be readable; qed;"),
		)
		.expect("File should be parseable; qed;");

		let failed_call = find_highest_pallet_index(&ast);
		assert!(
			matches!(failed_call, Err(msg) if msg.to_string() == "Unable to find the highest pallet index in runtime file")
		);
	}

	#[test]
	fn find_used_runtime_macro_with_construct_runtime_works_well() {
		let temp_dir = setup_template_construct_runtime_macro()
			.expect("Failed to setup template and instantiate");

		let ast = syn::parse_file(
			&std::fs::read_to_string(temp_dir.path().join("runtime").join("src").join("lib.rs"))
				.expect("File should be readable; qed;"),
		)
		.expect("File should be parseable; qed;");

		let used_macro =
			find_used_runtime_macro(&ast).expect("find_used_runtime_macro is supposed to be Ok");

		assert_eq!(used_macro, RuntimeUsedMacro::ConstructRuntime);
	}

	#[test]
	fn find_used_runtime_macro_with_runtime_macro_works_well() {
		let temp_dir =
			setup_template_runtime_v2_macro().expect("Failed to setup template and instantiate");

		let ast = syn::parse_file(
			&std::fs::read_to_string(temp_dir.path().join("runtime").join("src").join("lib.rs"))
				.expect("File should be readable; qed;"),
		)
		.expect("File should be parseable; qed;");

		let used_macro =
			find_used_runtime_macro(&ast).expect("find_used_runtime_macro is supposed to be Ok");

		assert_eq!(used_macro, RuntimeUsedMacro::Runtime);
	}

	#[test]
	fn find_used_runtime_macro_fails_if_input_isnt_runtime_file() {
		let temp_dir =
			setup_template_runtime_v2_macro().expect("Failed to setup template and instantiate");

		let ast = syn::parse_file(
			&std::fs::read_to_string(
				temp_dir.path().join("runtime").join("src").join("configs").join("mod.rs"),
			)
			.expect("File should be readable; qed;"),
		)
		.expect("File should be parseable; qed;");

		let failed_call = find_used_runtime_macro(&ast);

		assert!(
			matches!(failed_call, Err(msg) if msg.to_string() == "Unable to find a runtime declaration in runtime file")
		);
	}

	#[test]
	fn compute_pallet_related_paths_works() {
		let original_path = PathBuf::from("test");
		let paths = compute_pallet_related_paths(&original_path);

		assert_eq!(paths.runtime_lib_path, PathBuf::from("test/src/lib.rs"));
		assert_eq!(paths.configs_rs_path, PathBuf::from("test/src/configs.rs"));
		assert_eq!(paths.configs_folder_path, PathBuf::from("test/src/configs"));
		assert_eq!(paths.configs_mod_path, PathBuf::from("test/src/configs/mod.rs"));
	}

	#[test]
	fn create_new_pallet_impl_path_structure_configs_mod_template() {
		let temp_dir = setup_template_construct_runtime_macro()
			.expect("Failed to setup template and instantiate");

		let paths = compute_pallet_related_paths(&temp_dir.path().join("runtime"));

		let pallet_config_path = paths.configs_folder_path.join("test.rs");
		let pallet_name = "test";

		let mut rollback = Rollback::default();

		assert!(!pallet_config_path.exists());
		let configs_mod_before = std::fs::read_to_string(&paths.configs_mod_path).unwrap();

		rollback = create_new_pallet_impl_path_structure(
			rollback,
			&paths,
			&pallet_config_path,
			pallet_name,
		)
		.expect("Failed to create new pallet impl path structure");

		rollback.commit().expect("Failed to commit changes");

		let configs_mod_after = std::fs::read_to_string(&paths.configs_mod_path).unwrap();

		let configs_mod_diff = TextDiff::from_lines(&configs_mod_before, &configs_mod_after);

		let expected_inserted_lines = vec!["mod test;\n"];
		let mut inserted_lines = vec![];

		for change in configs_mod_diff.iter_all_changes() {
			match change.tag() {
				ChangeTag::Delete => panic!("no deletion expected"),
				ChangeTag::Insert => inserted_lines.push(change.value()),
				_ => (),
			}
		}

		assert!(pallet_config_path.exists());
		assert_eq!(expected_inserted_lines, inserted_lines);
	}

	#[test]
	fn create_new_pallet_impl_path_structure_configs_file_template() {
		let temp_dir = setup_template_construct_runtime_macro()
			.expect("Failed to setup template and instantiate");

		let paths = compute_pallet_related_paths(&temp_dir.path().join("runtime"));

		let pallet_config_path = &paths.configs_folder_path.join("test.rs");
		let pallet_name = "test";

		let mut rollback = Rollback::default();

		// Create a configs.rs file at the runtime level and delete the mod.rs file, to get a
		// template where the file configs.rs exists and then is used as the configs module root
		std::fs::remove_file(&paths.configs_mod_path).unwrap();
		std::fs::File::create(&paths.configs_rs_path).unwrap();

		assert!(!pallet_config_path.exists());
		let configs_rs_before = std::fs::read_to_string(&paths.configs_rs_path).unwrap();

		rollback = create_new_pallet_impl_path_structure(
			rollback,
			&paths,
			&pallet_config_path,
			pallet_name,
		)
		.expect("Failed to create new pallet impl path structure");

		rollback.commit().expect("Failed to commit changes");

		let configs_rs_after = std::fs::read_to_string(&paths.configs_rs_path).unwrap();

		let configs_rs_diff = TextDiff::from_lines(&configs_rs_before, &configs_rs_after);

		let expected_inserted_lines = vec!["mod test;\n"];
		let mut inserted_lines = vec![];

		for change in configs_rs_diff.iter_all_changes() {
			match change.tag() {
				ChangeTag::Delete => panic!("no deletion expected"),
				ChangeTag::Insert => inserted_lines.push(change.value()),
				_ => (),
			}
		}

		assert!(pallet_config_path.exists());
		assert_eq!(expected_inserted_lines, inserted_lines);
	}

	#[test]
	fn create_new_pallet_impl_path_structure_without_configs_template() {
		let temp_dir = setup_template_construct_runtime_macro()
			.expect("Failed to setup template and instantiate");

		let paths = compute_pallet_related_paths(&temp_dir.path().join("runtime"));

		let pallet_config_path = &paths.configs_folder_path.join("test.rs");
		let pallet_name = "test";

		let mut rollback = Rollback::default();

		// Remove configs from the template and clean the lib path (the only interesting thing here
		// is that pub mod configs; is added to that file).
		std::fs::remove_dir_all(&paths.configs_folder_path).unwrap();
		std::fs::File::create(&paths.runtime_lib_path).unwrap();

		assert!(!pallet_config_path.exists());
		let runtime_lib_before = std::fs::read_to_string(&paths.runtime_lib_path).unwrap();

		rollback = create_new_pallet_impl_path_structure(
			rollback,
			&paths,
			&pallet_config_path,
			pallet_name,
		)
		.expect("Failed to create new pallet impl path structure");

		rollback.commit().expect("Failed to commit changes");

		let runtime_lib_after = std::fs::read_to_string(&paths.runtime_lib_path).unwrap();

		let runtime_lib_diff = TextDiff::from_lines(&runtime_lib_before, &runtime_lib_after);

		let expected_inserted_lines = vec!["pub mod configs;\n"];
		let mut inserted_lines = vec![];

		for change in runtime_lib_diff.iter_all_changes() {
			match change.tag() {
				ChangeTag::Delete => panic!("no deletion expected"),
				ChangeTag::Insert => inserted_lines.push(change.value()),
				_ => (),
			}
		}

		assert!(pallet_config_path.exists());
		assert!(&paths.configs_folder_path.is_dir());
		assert_eq!(std::fs::read_to_string(&paths.configs_rs_path).unwrap(), "mod test;\n");
		assert_eq!(expected_inserted_lines, inserted_lines);
	}
}
