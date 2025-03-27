// SPDX-License-Identifier: GPL-3.0

#[cfg(test)]
mod tests;

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};

use clap::{Args, Subcommand};
use cliclack::{confirm, input, multiselect, outro, outro_cancel};
use fs_rollback::Rollback;
use pop_common::{
	capitalize_str, find_pallet_runtime_path, rust_writer_helpers,
	rust_writer_helpers::RuntimeUsedMacro,
};
use pop_parachains::{
	create_pallet_template, TemplatePalletConfig, TemplatePalletConfigCommonTypes,
	TemplatePalletOptions, TemplatePalletStorageTypes,
};
use proc_macro2::Span;
use rust_writer::{
	ast::{
		finder::{Finder, ToFind},
		implementors::{ItemToFile, ItemToMod, TokenStreamToMacro},
		mutator::{Mutator, ToMutate},
	},
	preserver::Preserver,
};
use rustilities::manifest::{ManifestDependencyConfig, ManifestDependencyOrigin};
use std::{fs, path::PathBuf};
use strum::{EnumMessage, IntoEnumIterator};
use syn::{parse_quote, parse_str, Ident, Type};

fn after_help_simple() -> &'static str {
	r#"Examples:
        pop new pallet 
            -> Will create a simple pallet, you'll have to choose your pallet name.
        pop new pallet my-pallet
            -> Will automatically create a pallet called my-pallet in the current directory.
        pop new pallet pallets/my-pallet 
            -> Will automatically create a pallet called my pallet in the directory ./pallets
        pop new pallet advanced 
            -> Will unlock the advanced mode. pop new pallet advanced --help for further info.
    "#
}

fn after_help_advanced() -> &'static str {
	r#"
        Examples:
            pop new pallet my-pallet advanced
                -> If no [OPTIONS] are specified, the interactive advanced mode is launched.
            pop new pallet my-pallet advanced --config-common-types runtime-origin currency --storage storage-value storage-map -d
                -> Using some [OPTIONS] will execute the non-interactive advanced mode.
    "#
}

#[derive(Args)]
#[command(after_help= after_help_simple())]
pub struct NewPalletCommand {
	#[arg(help = "Name of the pallet")]
	pub(crate) name: Option<String>,
	#[arg(short, long, help = "Name of authors", default_value = "Anonymous")]
	pub(crate) authors: Option<String>,
	#[arg(short, long, help = "Pallet description", default_value = "Frame Pallet")]
	pub(crate) description: Option<String>,
	#[arg(
		long,
		help = "If your pallet is created in a workspace containing a runtime, Pop-Cli will place the impl blocks for your pallets' Config traits inside a dedicated file under configs directory. Use this argument to point to other path."
	)]
	pub(crate) pallet_impl_path: Option<PathBuf>,
	#[command(subcommand)]
	pub(crate) mode: Option<Mode>,
}

#[derive(Subcommand)]
pub enum Mode {
	/// Advanced mode enables more detailed customization of pallet development.
	Advanced(AdvancedMode),
}

#[derive(Args)]
#[command(after_help = after_help_advanced())]
pub struct AdvancedMode {
	#[arg(short, long, value_enum, num_args(0..), help = "Add common types to your config trait from the CLI.")]
	pub(crate) config_common_types: Vec<TemplatePalletConfigCommonTypes>,
	#[arg(short, long, help = "Use a default configuration for your config trait.")]
	pub(crate) default_config: bool,
	#[arg(short, long, value_enum, num_args(0..), help = "Add storage items to your pallet from the CLI.")]
	pub(crate) storage: Vec<TemplatePalletStorageTypes>,
	#[arg(short, long, help = "Add a genesis config to your pallet.")]
	pub(crate) genesis_config: bool,
	#[arg(short = 'o', long, help = "Add a custom origin to your pallet.")]
	pub(crate) custom_origin: bool,
}

impl NewPalletCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Generate a pallet")?;

		let mut pallet_default_config = false;
		let mut pallet_common_types = Vec::new();
		let mut pallet_storage = Vec::new();
		let mut pallet_genesis = false;
		let mut pallet_custom_origin = false;

		if let Some(Mode::Advanced(advanced_mode_args)) = &self.mode {
			if advanced_mode_args.config_common_types.is_empty() &&
				advanced_mode_args.storage.is_empty() &&
				!(advanced_mode_args.genesis_config ||
					advanced_mode_args.default_config ||
					advanced_mode_args.custom_origin)
			{
				Cli.info("Generate the pallet's config trait.")?;

				pallet_common_types = multiselect_pick!(TemplatePalletConfigCommonTypes, "Are you interested in adding one of these types and their usual configuration to your pallet?");
				Cli.info("Generate the pallet's storage.")?;

				pallet_storage = multiselect_pick!(
					TemplatePalletStorageTypes,
					"Are you interested in adding some of those storage items to your pallet?"
				);

				// If there's no common types, default_config is excluded from the multiselect
				let boolean_options = if pallet_common_types.is_empty() {
					multiselect_pick!(
                        TemplatePalletOptions,
                        "Are you interested in adding one of these types and their usual configuration to your pallet?",
                        [TemplatePalletOptions::DefaultConfig]
				    )
				} else {
					multiselect_pick!(
                        TemplatePalletOptions,
                        "Are you interested in adding one of these types and their usual configuration to your pallet?"
                    )
				};

				pallet_default_config =
					boolean_options.contains(&TemplatePalletOptions::DefaultConfig);
				pallet_genesis = boolean_options.contains(&TemplatePalletOptions::GenesisConfig);
				pallet_custom_origin =
					boolean_options.contains(&TemplatePalletOptions::CustomOrigin);
			} else {
				pallet_common_types.clone_from(&advanced_mode_args.config_common_types);
				pallet_default_config = advanced_mode_args.default_config;
				if pallet_common_types.is_empty() && pallet_default_config {
					return Err(anyhow::anyhow!(
						"Specify at least a config common type to use default config."
					));
				}
				pallet_storage.clone_from(&advanced_mode_args.storage);
				pallet_genesis = advanced_mode_args.genesis_config;
				pallet_custom_origin = advanced_mode_args.custom_origin;
			}
		};

		let pallet_path = if let Some(path) = self.name {
			PathBuf::from(path)
		} else {
			let path: String = input("Where should your project be created?")
				.placeholder("./template")
				.default_input("./template")
				.interact()?;
			PathBuf::from(path)
		};

		// If the user has introduced something like pallets/my_pallet, probably it refers to
		// ./pallets/my_pallet. We need to transform this path, as otherwise the Cargo.toml won't be
		// detected and the pallet won't be added to the workspace.
		let pallet_path = rustilities::paths::prefix_with_current_dir(pallet_path);

		// Determine if the pallet is being created inside a workspace
		let workspace_toml = rustilities::manifest::find_workspace_manifest(&pallet_path);

		if pallet_path.exists() {
			if !confirm(format!(
				"\"{}\" directory already exists. Would you like to remove it?",
				pallet_path.display()
			))
			.interact()?
			{
				outro_cancel(format!(
					"Cannot generate pallet until \"{}\" directory is removed.",
					pallet_path.display()
				))?;
				return Ok(());
			}
			fs::remove_dir_all(pallet_path.clone())?;
		}
		let spinner = cliclack::spinner();
		spinner.start("Generating pallet...");
		create_pallet_template(
			pallet_path.clone(),
			TemplatePalletConfig {
				authors: self.authors.clone().expect("default values"),
				description: self.description.clone().expect("default values"),
				pallet_in_workspace: workspace_toml.is_some(),
				pallet_advanced_mode: self.mode.is_some(),
				pallet_default_config,
				pallet_common_types: pallet_common_types.clone(),
				pallet_storage,
				pallet_genesis,
				pallet_custom_origin,
			},
		)?;

		// If the pallet has been created inside a workspace containing a runtime, add the
		// pallet to that runtime.
		if let Some(runtime_path) = find_pallet_runtime_path(&pallet_path) {
			spinner.set_message("Adding the pallet to your runtime...");

			let pallet_crate_name =
				rustilities::manifest::find_crate_name(&pallet_path.join("Cargo.toml"))
					.unwrap_or("pallet".to_owned());

			let runtime_lib_path = runtime_path.join("src").join("lib.rs");
			let runtime_manifest =
				rustilities::manifest::find_innermost_manifest(&runtime_lib_path)
					.expect("Runtime is a crate, so it contains a manifest; qed;");

			let pallet_ident = Ident::new(
				&capitalize_str(
					&pallet_crate_name
						.split("pallet-")
						.last()
						.ok_or(anyhow::anyhow! {
						"Pallet crates are supposed to be called pallet-something."})?
						.replace("-", ""),
				),
				Span::call_site(),
			);
			let pallet_type = parse_str::<Type>(&pallet_crate_name.replace("-", "_"))?;

			let pallet_name =
				pallet_crate_name.splitn(2, '-').nth(1).unwrap_or("pallet").to_string();

			let (runtime_lib_path, configs_rs_path, configs_folder_path, configs_mod_path) =
				rust_writer_helpers::compute_pallet_related_paths(&runtime_path);
			let pallet_config_path = configs_folder_path.join(format!("{}.rs", pallet_name));

			let mut rollback = Rollback::default();

			if let Some(ref pallet_impl_path) = self.pallet_impl_path {
				rollback.note_file(pallet_impl_path)?;
			}

			rollback.note_file(&runtime_manifest)?;
			rollback.note_file(&runtime_lib_path)?;

			let roll_pallet_impl_path = match self.pallet_impl_path {
				Some(ref pallet_impl_path) => rollback
					.get_noted_file(&pallet_impl_path)
					.expect("The file has been noted above;qed;"),
				None => {
					rollback = rust_writer_helpers::compute_new_pallet_impl_path(
						rollback,
						&runtime_lib_path,
						&configs_rs_path,
						&configs_folder_path,
						&configs_mod_path,
						&pallet_config_path,
						&pallet_name,
					)?;

					rollback
						.get_new_file(&pallet_config_path)
						.expect("compute_new_pallet_impl_path noted this file; qed;")
				},
			};

			let roll_runtime_lib_path = rollback
				.get_noted_file(&runtime_lib_path)
				.expect("This file is noted by the rollback; qed;");
			let roll_manifest = rollback
				.get_noted_file(&runtime_manifest)
				.expect("This file is noted by the rollback; qed;");

			// Add the pallet to the runtime module
			let construct_runtime_preserver = Preserver::new("construct_runtime!");
			let mod_runtime_preserver = Preserver::new("mod runtime");
			let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
				roll_runtime_lib_path,
				&[&construct_runtime_preserver, &mod_runtime_preserver],
			)?;

			// Parse the runtime to find which of the runtime macros is being used and the highest
			// pallet index used (if needed, otherwise 0).
			let used_macro = rust_writer_helpers::find_used_runtime_macro(&preserved_ast)?;

			let pallet_added_to_runtime = match used_macro {
				RuntimeUsedMacro::Runtime => {
					let highest_index =
						rust_writer_helpers::find_highest_pallet_index(&preserved_ast)?;
					let pallet_to_runtime_implementor: ItemToMod = (
						"runtime",
						parse_quote! {
							///TEMP_DOC
							#[runtime::pallet_index(#highest_index)]
							pub type #pallet_ident = #pallet_type;
						},
					)
						.into();

					let mut finder = Finder::default().to_find(&pallet_to_runtime_implementor);
					let pallet_already_present = finder.find(&preserved_ast);
					if !pallet_already_present {
						let mut mutator =
							Mutator::default().to_mutate(&pallet_to_runtime_implementor);
						mutator.mutate(&mut preserved_ast)?;
						rust_writer::preserver::resolve_preserved(
							&preserved_ast,
							roll_runtime_lib_path,
						)
					} else {
						Ok(())
					}
				},
				RuntimeUsedMacro::ConstructRuntime => {
					let pallet_to_construct_runtime_implementor: TokenStreamToMacro = (
						parse_quote!(construct_runtime),
						Some(parse_quote!(Runtime)),
						parse_quote!(#pallet_ident: #pallet_type,),
					)
						.into();
					let mut finder =
						Finder::default().to_find(&pallet_to_construct_runtime_implementor);
					let pallet_already_present = finder.find(&preserved_ast);
					if !pallet_already_present {
						let mut mutator =
							Mutator::default().to_mutate(&pallet_to_construct_runtime_implementor);
						mutator.mutate(&mut preserved_ast)?;
						rust_writer::preserver::resolve_preserved(
							&preserved_ast,
							roll_runtime_lib_path,
						)
					} else {
						Ok(())
					}
				},
			};

			// Update the crate's manifest to add the pallet crate
			let relative_local_path = pathdiff::diff_paths(
				&pallet_path,
				&runtime_manifest
					.parent()
					.expect("A file's always contained inside a directory; qed;"),
			)
			.unwrap_or(pallet_path.clone());
			let crate_added_to_dependencies = rustilities::manifest::add_crate_to_dependencies(
				roll_manifest,
				&pallet_crate_name,
				ManifestDependencyConfig::new(
					ManifestDependencyOrigin::local(&relative_local_path),
					false,
					vec![],
					false,
				),
			);

			// Add pallet's impl block
			let mut preserved_ast =
				rust_writer::preserver::preserve_and_parse(roll_pallet_impl_path, &[])?;

			let (types, values) = if pallet_default_config {
				(Vec::new(), Vec::new())
			} else {
				let types: Vec<Ident> = pallet_common_types
					.clone()
					.iter()
					.map(|type_| {
						Ident::new(type_.get_message().unwrap_or_default(), Span::call_site())
					})
					.collect();
				let values: Vec<Type> =
					pallet_common_types.iter().map(|type_| type_.common_value()).collect();
				(types, values)
			};

			let pallet_impl_block_implementor = ItemToFile {
				item: parse_quote! {
					///TEMP_DOC
					impl #pallet_type::Config for Runtime{
						#(
						  type #types = #values;
						 )*
					}
				},
			};

			let mut mutator = Mutator::default().to_mutate(&pallet_impl_block_implementor);

			let pallet_impl_block_added = mutator.mutate(&mut preserved_ast);

			rust_writer::preserver::resolve_preserved(&preserved_ast, roll_pallet_impl_path)?;

			// If some of these results are Err, that doesn't mean that everything went wrong, only
			// that the pallet wasn't included into the runtime. But the pallet was indeed
			// created, so we cannot return an error here
			match (pallet_added_to_runtime, crate_added_to_dependencies, pallet_impl_block_added) {
				(Ok(_), Ok(_), Ok(_)) => rollback.commit()?,
				_ => {
					Cli.warning(
						"Your pallet has been created but it couldn't be added to your runtime.",
					)?;
				},
			}
		}

		// If the pallet has been created inside a workspace, add it to that workspace
		if let Some(mut workspace_toml) =
			rustilities::manifest::find_workspace_manifest(&pallet_path)
		{
			pop_common::add_crate_to_workspace(&workspace_toml, &pallet_path)?;
			workspace_toml.pop();
			rustilities::fmt::format_dir(&workspace_toml)?;
		} else {
			rustilities::fmt::format_dir(&pallet_path)?;
		}

		spinner.stop("Generation complete");
		outro(format!(
			"cd into \"{}\" and enjoy hacking! 🚀",
			pallet_path
				.to_str()
				.expect("If the path isn't valid, create_pallet_template detects it; qed")
		))?;
		Ok(())
	}
}
