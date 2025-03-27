// SPDX-License-Identifier: GPL-3.0

#[cfg(test)]
mod tests;

mod common_types;

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};
use clap::{error::ErrorKind, Args, Command};
use cliclack::multiselect;
use fs_rollback::Rollback;
use pop_common::{manifest, rust_writer_helpers::DefaultConfigType};
use rust_writer::{
	ast::{
		finder,
		finder::{Finder, ToFind},
		implementors::{ItemToFile, ItemToImpl, ItemToMod, ItemToTrait},
		mutator,
		mutator::{Mutator, ToMutate},
	},
	preserver::Preserver,
};
use std::{env, fs, path::PathBuf};
use strum::{EnumMessage, IntoEnumIterator};
use syn::{parse_quote, visit_mut::VisitMut};

#[mutator(ItemToImpl<'a>, ItemToImpl<'a>, ItemToImpl<'a>, ItemToImpl<'a>)]
#[finder(ItemToImpl<'a>, ItemToImpl<'a>, ItemToImpl<'a>, ItemToImpl<'a>)]
#[impl_from]
struct DefaultConfigsImplementor;

#[derive(Args, Debug, Clone)]
pub struct AddConfigTypeCommand {
	#[arg(short, long, value_enum, num_args(1..), required = false, help = "The types you want to include in your pallet.")]
	pub(crate) types: Vec<common_types::CommonTypes>,
	#[arg(
		short,
		long,
		help = "Pop-CLI will add the config type to the current directory lib if there's one. Use this argument to point to other path."
	)]
	pub(crate) pallet_path: Option<PathBuf>,
	#[arg(
		long,
		help = "If your pallet is part of a workspace containing a runtime, Pop-Cli will look for the impl block in configs/your_pallet_name.rs or in the runtime lib file to add the new type. Use this argument to point to other path."
	)]
	pub(crate) pallet_impl_path: Option<PathBuf>,
}

impl AddConfigTypeCommand {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Add a new type to your pallet")?;
		let mut cmd = Command::new("");

		let pallet_path = if let Some(path) = &self.pallet_path {
			rustilities::paths::prefix_with_current_dir(path)
		} else {
			// If not provided, use the working dir
			let working_dir = match env::current_dir() {
				Ok(working_dir) => working_dir,
				_ => cmd.error(ErrorKind::Io, "Cannot modify the working crate").exit(),
			};
			rustilities::paths::prefix_with_current_dir(working_dir)
		};

		let src = pallet_path.join("src");
		// Check that the path correspond to a pallet using that the file lib.rs always contains the
		// line #[pallet::pallet].
		let pallet_lib_path = src.join("lib.rs");
		if !pallet_lib_path.is_file() {
			cmd.error(
				ErrorKind::InvalidValue,
				"Make sure that the used path correspond to a pallet crate.",
			)
			.exit();
		}
		let lib_content = fs::read_to_string(&pallet_lib_path)?;
		if !lib_content.contains("#[pallet::pallet]") {
			cmd.error(
				ErrorKind::InvalidValue,
				"Make sure that the used path correspond to a pallet crate.",
			)
			.exit();
		}

		let mut types = if self.types.is_empty() {
			multiselect_pick!(
				common_types::CommonTypes,
				"Select the types you want to include in your pallet"
			)
		} else {
			self.types
		};

		types = common_types::complete_dependencies(types);

		let using_default_config = lib_content.contains("pub mod config_preludes");
		let pallet_crate_name =
			rustilities::manifest::find_crate_name(&pallet_path.join("Cargo.toml"))
				.ok_or(anyhow::anyhow!("Couldn't determine the pallet's crate name"))?;
		let config_preludes_path = pallet_path.join("src").join("config_preludes.rs");
		let runtime_path = manifest::find_pallet_runtime_path(&pallet_path);
		let pallet_mock_path = pallet_path.join("src").join("mock.rs");

		let spinner = cliclack::spinner();
		spinner.start("Updating pallet's config trait...");

		let pallet_impl_path = if let Some(ref runtime_path) = runtime_path {
			match self.pallet_impl_path {
				Some(_) => self.pallet_impl_path.clone(),
				_ => Some(manifest::get_pallet_impl_path(
					&runtime_path,
					&pallet_crate_name.splitn(2, '-').nth(1).unwrap_or("pallet").to_owned(),
				)?),
			}
		} else {
			None
		};

		let mut rollback = Rollback::with_capacity(4, 0, 0);

		rollback.note_file(&pallet_lib_path)?;
		rollback.note_file(&pallet_mock_path)?;
		// This may be Err cause the config preludes file may not exist. And it's perfect
		let _ = rollback.note_file(&config_preludes_path);
		// Note pallet_impl_path if needed
		if let Some(ref pallet_impl_path) = pallet_impl_path {
			rollback.note_file(&pallet_impl_path)?;
		}

		let roll_pallet_lib_path = rollback
			.get_noted_file(&pallet_lib_path)
			.expect("The file has been noted above; qed;");
		let roll_pallet_mock_path = rollback
			.get_noted_file(&pallet_mock_path)
			.expect("The file has been noted above; qed;");

		for type_ in types {
			let use_preserver = Preserver::new("use");
			let pub_use_preserver = Preserver::new("pub use");
			let mut mod_pallet_and_config_trait_preserver = Preserver::new("pub mod pallet");
			mod_pallet_and_config_trait_preserver.add_inners(&["pub trait Config"]);

			let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
				roll_pallet_lib_path,
				&[&use_preserver, &pub_use_preserver, &mod_pallet_and_config_trait_preserver],
			)?;

			for use_statement in type_.get_needed_use_statements() {
				let use_statement: ItemToFile = use_statement.into();
				let mut finder = Finder::default().to_find(&use_statement);
				let use_statement_used = finder.find(&preserved_ast);
				if !use_statement_used {
					let mut mutator = Mutator::default().to_mutate(&use_statement);
					mutator.mutate(&mut preserved_ast)?;
				}
			}

			for composite_enum in type_.get_needed_composite_enums() {
				let composite_enum_implementor: ItemToMod = ("pallet", composite_enum).into();
				let mut finder = Finder::default().to_find(&composite_enum_implementor);
				let composite_enum_used = finder.find(&preserved_ast);
				if !composite_enum_used {
					let mut mutator = Mutator::default().to_mutate(&composite_enum_implementor);
					mutator.mutate(&mut preserved_ast)?;
				}
			}

			let default_config = if using_default_config {
				type_.get_default_config()
			} else {
				// Here the inner element's irrelevant, so we place a simple type definition
				DefaultConfigType::Default { type_default_impl: parse_quote! {type marker = ();} }
			};

			let type_implementor: ItemToTrait =
				("Config", type_.get_type_definition(default_config.clone())).into();
			let mut finder = Finder::default().to_find(&type_implementor);
			let type_already_used = finder.find(&preserved_ast);

			if !type_already_used {
				let mut mutator = Mutator::default().to_mutate(&type_implementor);
				mutator.mutate(&mut preserved_ast)?;
			} else {
				return Err(anyhow::anyhow!(format!(
					"{} is already in use.",
					type_.get_message().expect("Message defined for all types; qed;")
				)));
			}

			rust_writer::preserver::resolve_preserved(&preserved_ast, roll_pallet_lib_path)?;

			match default_config {
				// Need to update default config
				DefaultConfigType::Default { type_default_impl } |
				DefaultConfigType::NoDefaultBounds { type_default_impl }
					if using_default_config =>
				{
					// If config_preludes isn't defined in its own file, we use the lib file.
					let file_path = if config_preludes_path.is_file() {
						rollback
							.get_noted_file(&config_preludes_path)
							.expect("config_preludes_path is file, so it's well noted; qed")
					} else {
						roll_pallet_lib_path
					};

					// Define preservers for the most common used struct names for default config.
					let preserver_testchain_config =
						Preserver::new("impl DefaultConfig for TestDefaultConfig");

					let preserver_solochain_config =
						Preserver::new("impl DefaultConfig for SolochainDefaultConfig");

					let preserver_relaychain_config =
						Preserver::new("impl DefaultConfig for RelayChainDefaultConfig");

					let preserver_parachain_config =
						Preserver::new("impl DefaultConfig for ParaChainDefaultConfig");

					let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
						file_path,
						&[
							&preserver_testchain_config,
							&preserver_solochain_config,
							&preserver_relaychain_config,
							&preserver_parachain_config,
						],
					)?;

					let default_config_implementor: DefaultConfigsImplementor = (
						(Some("DefaultConfig"), "TestDefaultConfig", type_default_impl.clone())
							.into(),
						(Some("DefaultConfig"), "TestDefaultConfig", type_default_impl.clone())
							.into(),
						(Some("DefaultConfig"), "TestDefaultConfig", type_default_impl.clone())
							.into(),
						(Some("DefaultConfig"), "TestDefaultConfig", type_default_impl.clone())
							.into(),
					)
						.into();

					let mut finder: DefaultConfigsImplementorFinderWrapper =
						Finder::default().to_find(&default_config_implementor).into();
					finder.find(&preserved_ast, None);
					let missing_indexes = finder.get_missing_indexes();
					if missing_indexes.is_some() {
						let mut mutator: DefaultConfigsImplementorMutatorWrapper =
							Mutator::default().to_mutate(&default_config_implementor).into();
						mutator.mutate(&mut preserved_ast, missing_indexes.as_deref())?;
						rust_writer::preserver::resolve_preserved(&preserved_ast, file_path)?;
					}
				},
				// Need to update runtimes
				_ => {
					let pallet_name = pallet_crate_name.replace("-", "_");
					let pallet_config_trait_impl = format!("impl {}::Config", pallet_name);
					let pallet_impl_preserver = Preserver::new(&pallet_config_trait_impl);

					let runtime_value_implementor: ItemToImpl =
						(Some("Config"), "Runtime", type_.get_common_runtime_value()).into();

					let mock_runtime_value_implementor: ItemToImpl =
						(Some("Config"), "Test", type_.get_common_runtime_value()).into();

					if let Some(ref impl_path) = pallet_impl_path {
						let roll_impl_path = rollback
							.get_noted_file(impl_path)
							.expect("The file has been noted above; qed;");

						let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
							roll_impl_path,
							&[&pallet_impl_preserver],
						)?;

						let mut finder = Finder::default().to_find(&runtime_value_implementor);
						let runtime_value_already_used = finder.find(&preserved_ast);
						if !runtime_value_already_used {
							let mut mutator =
								Mutator::default().to_mutate(&runtime_value_implementor);
							mutator.mutate(&mut preserved_ast)?;
							rust_writer::preserver::resolve_preserved(
								&preserved_ast,
								roll_impl_path,
							)?;
						}
					}

					let mut preserved_ast = rust_writer::preserver::preserve_and_parse(
						roll_pallet_mock_path,
						&[&pallet_impl_preserver],
					)?;

					let mut finder = Finder::default().to_find(&mock_runtime_value_implementor);
					let runtime_value_already_used = finder.find(&preserved_ast);
					if !runtime_value_already_used {
						let mut mutator =
							Mutator::default().to_mutate(&mock_runtime_value_implementor);
						mutator.mutate(&mut preserved_ast)?;
						rust_writer::preserver::resolve_preserved(
							&preserved_ast,
							roll_pallet_mock_path,
						)?;
					}
				},
			}
		}

		rollback.commit()?;

		if let Some(mut workspace_toml) =
			rustilities::manifest::find_workspace_manifest(&pallet_path)
		{
			workspace_toml.pop();
			rustilities::fmt::format_dir(&workspace_toml)?;
		} else {
			rustilities::fmt::format_dir(&pallet_path)?;
		}

		spinner.stop("Your types are ready to be used in your pallet 🚀");
		Ok(())
	}
}
