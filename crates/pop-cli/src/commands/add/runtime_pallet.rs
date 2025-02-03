// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};
use clap::{error::ErrorKind, Args, Command};
use cliclack::multiselect;
use pop_common::{
	find_workspace_toml, format_dir, manifest, prefix_with_current_dir_if_needed, rust_writer,
	Rollback,
};
use std::{
	env,
	path::PathBuf,
	sync::{Arc, Mutex},
};
use strum::{EnumMessage, IntoEnumIterator};

mod common_pallets;

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
			prefix_with_current_dir_if_needed(path.to_path_buf())
		} else {
			let working_dir = match env::current_dir() {
				Ok(working_dir) => working_dir,
				_ => cmd.error(ErrorKind::Io, "Cannot modify the working crate").exit(),
			};
			// Give the chance to use the command either from a workspace containing a runtime or
			// from a runtime crate if path not specified
			if working_dir.join("runtime").exists() {
				prefix_with_current_dir_if_needed(working_dir.join("runtime"))
			} else {
				prefix_with_current_dir_if_needed(working_dir)
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

		let mut handles = Vec::new();
		// Mutex over the memory shared across threads
		let mutex_pallet_impl_path = Arc::new(Mutex::new(self.pallet_impl_path));
		let mutex_runtime_path = Arc::new(Mutex::new(runtime_path.clone()));

		for pallet in pallets {
			let mutex_pallet_impl_path = Arc::clone(&mutex_pallet_impl_path);
			let mutex_runtime_path = Arc::clone(&mutex_runtime_path);
			handles.push(std::thread::spawn(move || -> Result<(), anyhow::Error> {
				let pallet_impl_path = mutex_pallet_impl_path
					.lock()
					.map_err(|e| anyhow::Error::msg(format!("{}", e)))?;
				let runtime_path =
					mutex_runtime_path.lock().map_err(|e| anyhow::Error::msg(format!("{}", e)))?;

				let runtime_lib_path = runtime_path.join("src").join("lib.rs");
				let runtime_manifest = manifest::find_crate_manifest(&runtime_lib_path)
					.expect("Runtime is a crate, so it contains a manifest; qed;");

				let mut rollback;
				let roll_runtime_lib_path;
				let pallet_impl_path = match *pallet_impl_path {
					Some(_) => {
						rollback = Rollback::with_capacity(3, 0, 0);
						roll_runtime_lib_path = rollback.note_file(&runtime_lib_path)?;
						pallet_impl_path
							.clone()
							.expect("The match arm guarantees this is Some; qed;")
					},
					None => {
						let (rollback_temp, runtime_lib_path_rolled) =
							manifest::compute_new_pallet_impl_path(
								&runtime_path,
								&pallet
									.get_crate_name()
									.splitn(2, '-')
									.nth(1)
									.unwrap_or("pallet")
									.to_string(),
							)?;
						rollback = rollback_temp;

						// The rollback created above may already contain a noted version of
						// runtime_lib_path and a new file which corresponds to the pallet
						// impl path
						roll_runtime_lib_path = if runtime_lib_path_rolled {
							rollback.noted_files().remove(0)
						} else {
							rollback.note_file(&runtime_lib_path)?
						};
						rollback.new_files().remove(0)
					},
				};

				let roll_pallet_impl_path = rollback.note_file(&pallet_impl_path)?;
				let roll_manifest = rollback.note_file(&runtime_manifest)?;

				// Add the pallet to the crate and to the runtime module
				let (rollback, _) =
					rollback.ok_or_rollback(rust_writer::add_pallet_to_runtime_module(
						&pallet.get_crate_name(),
						&roll_runtime_lib_path,
					))?;

				// Add the pallet impl block and its related use statements
				let (rollback, _) = rollback.ok_or_rollback(rust_writer::add_use_statements(
					&roll_pallet_impl_path,
					pallet.get_impl_needed_use_statements(),
				))?;

				let (rollback, _) =
					rollback.ok_or_rollback(rust_writer::add_pallet_impl_block_to_runtime(
						&pallet.get_crate_name(),
						&roll_pallet_impl_path,
						pallet.get_parameter_types(),
						pallet.get_config_types(),
						pallet.get_config_values(),
						pallet.get_default_config(),
					))?;

				// Update the crate's manifest to add the pallet crate
				let (rollback, _) =
					rollback.ok_or_rollback(manifest::add_crate_to_dependencies(
						&roll_manifest,
						&runtime_manifest,
						&pallet.get_crate_name(),
						manifest::types::CrateDependencie::External {
							version: pallet.get_version(),
						},
					))?;

				// At this point, we can commit the rollback
				rollback.commit();
				Ok(())
			}));
		}

		for handle in handles {
			let result = handle.join().expect("Unexpected error");
			if result.is_err() {
				Cli.warning("Some of the pallets weren't added to your runtime")?;
			}
		}

		if let Some(mut workspace_toml) = find_workspace_toml(&runtime_path) {
			workspace_toml.pop();
			format_dir(&workspace_toml)?;
		} else {
			format_dir(&runtime_path)?;
		}

		spinner.stop("Your runtime has been updated and it's ready to use 🚀");
		Ok(())
	}
}
