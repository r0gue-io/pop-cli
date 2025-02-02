// SPDX-License-Identifier: GPL-3.0

mod common_types;

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};
use clap::{error::ErrorKind, Args, Command};
use cliclack::multiselect;
use pop_common::{
	capitalize_str, find_workspace_toml, format_dir, manifest, prefix_with_current_dir_if_needed,
	rust_writer::{self, types::*},
};
use proc_macro2::Span;
use std::{
	env, fs,
	path::PathBuf,
	sync::{Arc, Mutex},
};
use strum::{EnumMessage, IntoEnumIterator};
use syn::{parse_quote, Ident};

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
			prefix_with_current_dir_if_needed(path.to_path_buf())
		} else {
			// If not provided, use the working dir
			let working_dir = match env::current_dir() {
				Ok(working_dir) => working_dir,
				_ => cmd.error(ErrorKind::Io, "Cannot modify the working crate").exit(),
			};
			prefix_with_current_dir_if_needed(working_dir)
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

		let mut handles = Vec::new();
		// Mutex the memory shared accross threads
		let mutex_pallet_path = Arc::new(Mutex::new(pallet_path.clone()));
		let mutex_pallet_impl_path = Arc::new(Mutex::new(self.pallet_impl_path));

		let spinner = cliclack::spinner();
		spinner.start("Updating pallet's config trait...");

		for type_ in types {
			let mutex_pallet_path = Arc::clone(&mutex_pallet_path);
			let mutex_pallet_impl_path = Arc::clone(&mutex_pallet_impl_path);
			handles.push(std::thread::spawn(move || -> Result<(), anyhow::Error> {
				let pallet_impl_path = mutex_pallet_impl_path
					.lock()
					.map_err(|e| anyhow::Error::msg(format!("{}", e)))?;
				let pallet_path =
					mutex_pallet_path.lock().map_err(|e| anyhow::Error::msg(format!("{}", e)))?;

				let pallet_lib_path = pallet_path.join("src").join("lib.rs");
				let pallet_crate_name = manifest::find_crate_name(&pallet_path.join("Cargo.toml"))?;
				let config_preludes_path = pallet_path.join("src").join("config_preludes.rs");
				let runtime_path = manifest::find_pallet_runtime_path(&pallet_path);

				rust_writer::add_use_statements(
					&pallet_lib_path,
					type_.get_needed_use_statements(),
				)?;

				rust_writer::add_composite_enums(
					&pallet_lib_path,
					type_.get_needed_composite_enums(),
				)?;

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
					&pallet_lib_path,
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
						// If config_preludes is defined in its own file, we pass it to
						// 'add_type_to_config_preludes", otherwise we pass lib.rs
						let file_path = if config_preludes_path.is_file() {
							&config_preludes_path
						} else {
							&pallet_lib_path
						};

						rust_writer::add_type_to_config_preludes(file_path, type_default_impl)?;
					},
					// Need to update runtimes
					_ => {
						let pallet_impl_path = if let Some(runtime_path) = runtime_path {
							match *pallet_impl_path {
								Some(_) => pallet_impl_path.clone(),
								_ => Some(manifest::get_pallet_impl_path(
									&runtime_path,
									&pallet_crate_name
										.splitn(2, '-')
										.nth(1)
										.unwrap_or("pallet")
										.to_owned(),
								)?),
							}
						} else {
							None
						};

						rust_writer::add_type_to_runtimes(
							&pallet_path,
							type_name_ident,
							type_.get_common_runtime_value(),
							pallet_impl_path,
						)?;
					},
				}

				Ok(())
			}));
		}

		for handle in handles {
			let result = handle.join().expect("Unexpected error");
			if result.is_err() {
				return result;
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
