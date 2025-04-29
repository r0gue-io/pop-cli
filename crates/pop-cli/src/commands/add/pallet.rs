// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	common::writer::{self, PalletConfigRelatedPaths, RuntimeUsedMacro},
};
use clap::{error::ErrorKind, Args, Command};
use common_pallets::CommonPallets;
use fs_rollback::Rollback;
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
	#[arg(long, short, help = "The pallet added to the runtime.")]
	pub(crate) pallet: Option<CommonPallets>,
	#[arg(
		short,
		long,
		help = "pop add pallet should be called from a runtime crate or from a workspace containing a runtime crate. If this command is called from somewhere else, this argument allows to specify the path to the runtime crate."
	)]
	pub(crate) runtime_path: Option<PathBuf>,
	#[arg(
		long,
		help = "pop add pallet will place the impl blocks for your pallets' Config traits inside a dedicated file under the configs directory. Use this argument to place them somewhere else."
	)]
	pub(crate) pallet_impl_path: Option<PathBuf>,
	#[arg(long, short, help = "The pallet version.")]
	pub(crate) version: Option<String>,
}

const INVALID_DIR_MSG: &str = "Make sure to run this command either in a runtime crate contained in a workspace, in the workspace itself or to specify the path to the runtime crate using -r.";

impl AddPalletCommand {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Add a new pallet to your runtime")?;
		let mut cmd = Command::new("");

		let (pallet, version) = match (self.pallet, self.version) {
			(Some(pallet), Some(version)) => (pallet, version),
			(None, None) => {
				let mut pallet_prompt =
					cliclack::select("Select a pallet to add to your runtime: ".to_owned());
				for (i, pallet) in CommonPallets::iter().enumerate() {
					if i == 0 {
						pallet_prompt = pallet_prompt.initial_value(pallet);
					}
					pallet_prompt = pallet_prompt.item(
						pallet,
						pallet.get_message().expect("all variants of CommonPallets have message; qed;"),
						pallet.get_detailed_message().expect("all variants of CommonPallets have detailed_message; qed;"),
					);
				}
				let mut version_prompt = cliclack::input("Which version should use your pallet?")
					.placeholder("1.0.0")
					.default_input("1.0.0");
		    (pallet_prompt.interact()?, version_prompt.interact()?)
			},
			_ => cmd
            .error(
                ErrorKind::Io,
                "If you specify pallet/version via the command line, both fields must be specified"
            )
            .exit(),
		};

		let runtime_path = if let Some(path) = &self.runtime_path {
			pop_common::helpers::prefix_with_current_dir_if_needed(&path)
		} else {
			let working_dir = match env::current_dir() {
				Ok(working_dir) => working_dir,
				_ => cmd.error(ErrorKind::Io, "Cannot modify the working crate").exit(),
			};
			// Give the chance to use the command either from a workspace containing a runtime or
			// from a runtime crate if path not specified
			if working_dir.join("runtime").exists() {
				pop_common::helpers::prefix_with_current_dir_if_needed(working_dir.join("runtime"))
			} else {
				pop_common::helpers::prefix_with_current_dir_if_needed(&working_dir)
			}
		};

		let spinner = cliclack::spinner();
		spinner.start("Updating runtime...");

		let pallet_name = pallet
			.get_message()
			.expect("All pallets in common_pallets::CommonPallets have a defined message; qed;");

		let pallet_config_related_paths = writer::compute_pallet_related_paths(&runtime_path);

		let PalletConfigRelatedPaths { runtime_lib_path, configs_folder_path, .. } =
			pallet_config_related_paths.clone();

		let runtime_lib_content = std::fs::read_to_string(&runtime_lib_path)?;

		if !runtime_lib_content.contains("construct_runtime!") &&
			!runtime_lib_content.contains("mod runtime")
		{
			cmd.error(ErrorKind::InvalidValue, INVALID_DIR_MSG).exit();
		}

		let pallet_config_path = configs_folder_path.join(format!("{}.rs", pallet_name));

		let mut rollback = Rollback::default();

		let runtime_manifest = rustilities::manifest::find_innermost_manifest(&runtime_path)
			.ok_or(anyhow::anyhow!(INVALID_DIR_MSG))?;

		let workspace_manifest = pop_common::find_workspace_toml(&runtime_path)
			.ok_or(anyhow::anyhow!(INVALID_DIR_MSG))?;

		rollback.note_file(&runtime_lib_path)?;

		rollback.note_file(&runtime_manifest)?;

		rollback.note_file(&workspace_manifest)?;

		if let Some(ref pallet_impl_path) = self.pallet_impl_path {
			// The impl path may be the runtime lib, so the path may be already noted.
			match rollback.note_file(pallet_impl_path) {
				Ok(()) => (),
				Err(fs_rollback::Error::AlreadyNoted(_)) => (),
				Err(err) => return Err(err.into()),
			}
		}

		let roll_pallet_impl_path = match self.pallet_impl_path {
			Some(ref pallet_impl_path) => rollback
				.get_noted_file(pallet_impl_path)
				.expect("The file has been noted above;qed;"),
			None => {
				rollback = writer::create_new_pallet_impl_path_structure(
					rollback,
					&pallet_config_related_paths,
					&pallet_config_path,
					&pallet_name,
				)?;

				rollback
					.get_new_file(&pallet_config_path)
					.expect("create_new_pallet_impl_path_structure noted this file; qed;")
			},
		};

		let roll_runtime_lib_path = rollback
			.get_noted_file(&runtime_lib_path)
			.expect("The file has been noted above; qed;");

		let roll_runtime_manifest = rollback
			.get_noted_file(&runtime_manifest)
			.expect("The file has been noted above; qed;");

		let roll_workspace_manifest = rollback
			.get_noted_file(&workspace_manifest)
			.expect("The file has been noted above; qed;");

		// Add the pallet to the runtime module
		let construct_runtime_preserver = Preserver::new("construct_runtime!");
		let mod_runtime_preserver = Preserver::new("mod runtime");
		let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
			roll_runtime_lib_path,
			&[&construct_runtime_preserver, &mod_runtime_preserver],
		)?;

		// Parse the runtime to find which of the runtime macros is being used and the highest
		// pallet index used (if needed, otherwise 0).
		let used_macro = writer::find_used_runtime_macro(&preserved_ast)?;
		match used_macro {
			RuntimeUsedMacro::Runtime => {
				let highest_index = writer::find_highest_pallet_index(&preserved_ast)?;
				let pallet_to_runtime_implementor: ItemToMod =
					("runtime", pallet.get_pallet_declaration_runtime_module(highest_index)).into();

				let mut finder = Finder::default().to_find(&pallet_to_runtime_implementor);
				if finder.find(&preserved_ast) {
					return Err(anyhow::anyhow!(format!(
						"{} is already in use.",
						pallet.get_crate_name()
					)));
				} else {
					let mut mutator = Mutator::default().to_mutate(&pallet_to_runtime_implementor);
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
				if finder.find(&preserved_ast) {
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
			if !finder.find(&preserved_ast) {
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

		// Update the manifests to add the pallet crate
		rustilities::manifest::add_crate_to_dependencies(
			roll_workspace_manifest,
			&pallet.get_crate_name(),
			ManifestDependencyConfig::new(
				ManifestDependencyOrigin::crates_io(&version),
				false,
				vec![],
				false,
			),
		)?;

		rustilities::manifest::add_crate_to_dependencies(
			roll_runtime_manifest,
			&pallet.get_crate_name(),
			ManifestDependencyConfig::new(
				ManifestDependencyOrigin::workspace(),
				false,
				vec![],
				false,
			),
		)?;

		pop_common::manifest::add_pallet_features_to_manifest(
			roll_runtime_manifest,
			pallet.get_crate_name(),
		)?;

		rollback.commit()?;

		if let Some(mut workspace_toml) = pop_common::manifest::find_workspace_toml(&runtime_path) {
			workspace_toml.pop();
			rustilities::fmt::format_dir(&workspace_toml)?;
		} else {
			rustilities::fmt::format_dir(&runtime_path)?;
		}

		spinner.stop("Your runtime has been updated and it's ready to use 🚀");
		Ok(())
	}
}
