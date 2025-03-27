// SPDX-License-Identifier: GPL-3.0

#[cfg(test)]
mod tests;

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};
use clap::{error::ErrorKind, Args, Command};
use cliclack::multiselect;
use fs_rollback::Rollback;
use pop_common::{
	manifest,
	rust_writer_helpers::{self, RuntimeUsedMacro},
};
use rust_writer::{
	ast::{
		finder,
		finder::{Finder, ToFind},
		implementors::{ItemToFile, ItemToMod, TokenStreamToMacro},
		mutator,
		mutator::{Mutator, ToMutate},
	},
	preserver::Preserver,
};
use rustilities::manifest::{ManifestDependencyConfig, ManifestDependencyOrigin};
use std::{env, path::PathBuf};
use strum::{EnumMessage, IntoEnumIterator};
use syn::{parse_quote, visit_mut::VisitMut};

mod common_pallets;

#[mutator(ItemToFile, ItemToFile)]
#[finder(ItemToFile, ItemToFile)]
#[impl_from]
struct PalletImplBlockImplementor;

#[derive(Args, Debug, Clone)]
pub struct AddPalletCommand {
	#[arg(short, long, value_enum, num_args(1..), required = false, help = "The pallets you want to include to your runtime.")]
	pub(crate) pallets: Vec<common_pallets::CommonPallets>,
	#[arg(short, long, help = "Specify the path to the runtime crate.")]
	pub(crate) runtime_path: Option<PathBuf>,
	#[arg(
		long,
		help = "Pop-Cli will place the impl blocks for your pallets' Config traits inside a dedicated file under configs directory. Use this argument to point to other path."
	)]
	pub(crate) pallet_impl_path: Option<PathBuf>,
}

impl AddPalletCommand {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Add a new pallet to your runtime")?;
		let mut cmd = Command::new("");

		let runtime_path = if let Some(path) = &self.runtime_path {
			rustilities::paths::prefix_with_current_dir(path)
		} else {
			let working_dir = match env::current_dir() {
				Ok(working_dir) => working_dir,
				_ => cmd.error(ErrorKind::Io, "Cannot modify the working crate").exit(),
			};
			// Give the chance to use the command either from a workspace containing a runtime or
			// from a runtime crate if path not specified
			if working_dir.join("runtime").exists() {
				rustilities::paths::prefix_with_current_dir(working_dir.join("runtime"))
			} else {
				rustilities::paths::prefix_with_current_dir(working_dir)
			}
		};

		if !manifest::is_runtime_crate(&runtime_path) {
			cmd.error(
				ErrorKind::InvalidValue,
				"Make sure to run this command either in a workspace containing a runtime crate/a runtime crate or to specify the path to the runtime crate using -r.",
			)
			.exit();
		}

		let spinner = cliclack::spinner();
		spinner.start("Updating runtime...");

		let pallets = if self.pallets.is_empty() {
			multiselect_pick!(
				common_pallets::CommonPallets,
				"Select the pallets you want to include in your runtime"
			)
		} else {
			self.pallets
		};

		let mut rollback = Rollback::default();

		let mut precomputed_pallet_config_paths = Vec::with_capacity(pallets.len());

		let (runtime_lib_path, configs_rs_path, configs_folder_path, configs_mod_path) =
			rust_writer_helpers::compute_pallet_related_paths(&runtime_path);

		let runtime_manifest = rustilities::manifest::find_innermost_manifest(&runtime_path)
			.expect("Runtime is a crate, so it contains a manifest; qed;");

		for pallet in pallets.iter() {
			let pallet_name =
				pallet.get_crate_name().splitn(2, '-').nth(1).unwrap_or("pallet").to_string();
			precomputed_pallet_config_paths
				.push(configs_folder_path.join(format!("{}.rs", pallet_name)));
		}

		rollback.note_file(&runtime_lib_path)?;
		rollback.note_file(&runtime_manifest)?;
		if let Some(ref pallet_impl_path) = self.pallet_impl_path {
			rollback.note_file(pallet_impl_path)?;
		}

		for (index, pallet) in pallets.iter().enumerate() {
			let pallet_name =
				pallet.get_crate_name().splitn(2, '-').nth(1).unwrap_or("pallet").to_string();

			let pallet_config_path = &precomputed_pallet_config_paths[index];

			let roll_pallet_impl_path = match self.pallet_impl_path {
				Some(ref pallet_impl_path) => rollback
					.get_noted_file(pallet_impl_path)
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
			match used_macro {
				RuntimeUsedMacro::Runtime => {
					let highest_index =
						rust_writer_helpers::find_highest_pallet_index(&preserved_ast)?;
					let pallet_to_runtime_implementor: ItemToMod =
						("runtime", pallet.get_pallet_declaration_runtime_module(highest_index))
							.into();

					let mut finder = Finder::default().to_find(&pallet_to_runtime_implementor);
					let pallet_already_present = finder.find(&preserved_ast);
					if pallet_already_present {
						return Err(anyhow::anyhow!(format!(
							"{} is already in use.",
							pallet.get_crate_name()
						)));
					} else {
						let mut mutator =
							Mutator::default().to_mutate(&pallet_to_runtime_implementor);
						mutator.mutate(&mut preserved_ast)?;
						rust_writer::preserver::resolve_preserved(
							&preserved_ast,
							roll_runtime_lib_path,
						)?;
					}
				},
				RuntimeUsedMacro::ConstructRuntime => {
					let pallet_to_construct_runtime_implementor: TokenStreamToMacro = (
						parse_quote!(construct_runtime),
						Some(parse_quote!(Runtime)),
						pallet.get_pallet_declaration_construct_runtime(),
					)
						.into();
					let mut finder =
						Finder::default().to_find(&pallet_to_construct_runtime_implementor);
					let pallet_already_present = finder.find(&preserved_ast);
					if pallet_already_present {
						return Err(anyhow::anyhow!(format!(
							"{} is already in use.",
							pallet.get_crate_name()
						)));
					} else {
						let mut mutator =
							Mutator::default().to_mutate(&pallet_to_construct_runtime_implementor);
						mutator.mutate(&mut preserved_ast)?;
						rust_writer::preserver::resolve_preserved(
							&preserved_ast,
							roll_runtime_lib_path,
						)?;
					}
				},
			}

			// Add the pallet impl block and its related use statements
			let use_preserver = Preserver::new("use");
			let pub_use_preserver = Preserver::new("pub use");

			let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
				roll_pallet_impl_path,
				&[&use_preserver, &pub_use_preserver],
			)?;

			for use_statement in pallet.get_impl_needed_use_statements() {
				let use_statement: ItemToFile = use_statement.into();
				let mut finder = Finder::default().to_find(&use_statement);
				let use_statement_used = finder.find(&preserved_ast);
				if !use_statement_used {
					let mut mutator = Mutator::default().to_mutate(&use_statement);
					mutator.mutate(&mut preserved_ast)?;
				}
			}

			let pallet_impl_block_implementor: PalletImplBlockImplementor = (
				ItemToFile { item: pallet.get_needed_parameter_types() },
				ItemToFile { item: pallet.get_needed_impl_block() },
			)
				.into();

			let mut mutator: PalletImplBlockImplementorMutatorWrapper =
				Mutator::default().to_mutate(&pallet_impl_block_implementor).into();

			mutator.mutate(&mut preserved_ast, None)?;

			rust_writer::preserver::resolve_preserved(&preserved_ast, roll_pallet_impl_path)?;

			// Update the crate's manifest to add the pallet crate
			rustilities::manifest::add_crate_to_dependencies(
				roll_manifest,
				&pallet.get_crate_name(),
				ManifestDependencyConfig::new(
					ManifestDependencyOrigin::crates_io(&pallet.get_version()),
					false,
					vec![],
					false,
				),
			)?;
		}

		rollback.commit()?;

		if let Some(mut workspace_toml) =
			rustilities::manifest::find_workspace_manifest(&runtime_path)
		{
			workspace_toml.pop();
			rustilities::fmt::format_dir(&workspace_toml)?;
		} else {
			rustilities::fmt::format_dir(&runtime_path)?;
		}

		spinner.stop("Your runtime has been updated and it's ready to use 🚀");
		Ok(())
	}
}
