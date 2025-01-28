// SPDX-License-Identifier: GPL-3.0

mod common_types;

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};
use clap::{error::ErrorKind, Args, Command};
use cliclack::multiselect;
use pop_common::{
	capitalize_str, find_workspace_toml, format_dir, prefix_with_current_dir_if_needed,
	rust_writer::{self, types::*},
};
use proc_macro2::Span;
use std::{fs, path::PathBuf};
use strum::{EnumMessage, IntoEnumIterator};
use syn::{parse_quote, Ident};

#[derive(Args, Debug, Clone)]
pub struct AddConfigTypeCommand {
	#[arg(short, long, required = true, help = "Specify the path to the pallet crate.")]
	pub(crate) pallet_path: PathBuf,
	#[arg(short, long, value_enum, num_args(1..), required = false, help = "The types you want to include in your pallet.")]
	pub(crate) types: Vec<common_types::CommonTypes>,
	#[arg(
		long,
		help = "If your pallet is included in a runtime, Pop-Cli will look for the impl block for your pallet's Config trait inside configs/mod.rs or lib.rs in the runtime crate by default in order to add the new type to the runtime. If your impl block is in another path, use this option to specify it."
	)]
	pub(crate) runtime_impl_path: Option<PathBuf>,
}

impl AddConfigTypeCommand {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Add a new type to your pallet")?;
		let mut cmd = Command::new("");
		let pallet_path = prefix_with_current_dir_if_needed(self.pallet_path);
		let src = pallet_path.join("src");
		// Check that the path correspond to a pallet using that the file lib.rs always contains the
		// line #[pallet::pallet].
		let lib_path = src.join("lib.rs");
		if !lib_path.is_file() {
			cmd.error(
				ErrorKind::InvalidValue,
				"Make sure that the used path correspond to a pallet crate.",
			)
			.exit();
		}
		let lib_content = fs::read_to_string(&lib_path)?;
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

		let spinner = cliclack::spinner();
		spinner.start("Updating pallet's config trait...");

		for type_ in types {
			rust_writer::add_use_statements(&lib_path, type_.get_needed_use_statements())?;

			rust_writer::add_composite_enums(&lib_path, type_.get_needed_composite_enums())?;

			let type_name_ident = Ident::new(
				&capitalize_str(type_.get_message().unwrap_or_default()),
				Span::call_site(),
			);
			let default_config = if using_default_config {
				type_.get_default_config()
			} else {
				// Here the inner element's irrelevant, so we place a simple type definition
				DefaultConfigType::Default { type_default_impl: parse_quote! {type A = ();} }
			};
			// Update the config trait in lib.rs
			rust_writer::update_config_trait(
				&lib_path,
				type_name_ident.clone(),
				type_.get_common_trait_bounds(),
				&default_config,
			)?;

			match default_config {
				// Need to update default config
				DefaultConfigType::Default { type_default_impl } |
				DefaultConfigType::NoDefaultBounds { type_default_impl }
					if using_default_config =>
				{
					spinner.set_message(
						"Adding your type's default value to the pallet's config preludes...",
					);
					// If config_preludes is defined in its own file, we pass it to
					// 'add_type_to_config_preludes", otherwise we pass lib.rs
					let config_preludes_path = src.join("config_preludes.rs");
					let file_path = if config_preludes_path.is_file() {
						&config_preludes_path
					} else {
						&lib_path
					};

					rust_writer::add_type_to_config_preludes(file_path, type_default_impl)?;
				},
				// Need to update runtimes
				_ => {
					spinner.set_message("Adding your type to pallet's related runtimes...");
					rust_writer::add_type_to_runtimes(
						&pallet_path,
						type_name_ident,
						type_.get_common_runtime_value(),
						self.runtime_impl_path.as_deref(),
					)?;
				},
			}
		}

		if let Some(mut workspace_toml) = find_workspace_toml(&pallet_path) {
			workspace_toml.pop();
			format_dir(&workspace_toml)?;
		} else {
			format_dir(&pallet_path)?;
		}
		spinner.stop("Your type is ready to be used in your pallet 🚀");
		Ok(())
	}
}
