// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};
use clap::{error::ErrorKind, Args, Command};
use cliclack::multiselect;
use pop_common::{
	find_workspace_toml, format_dir, get_pallet_impl_path, manifest,
	prefix_with_current_dir_if_needed, rust_writer,
};
use std::{env, path::PathBuf};
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
		help = "Pop-Cli will place the impl blocks for your pallets' Config traits inside a dedicated file under configs directory. Use this argument to point to another path."
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

		let src = runtime_path.join("src");
		let lib_path = src.join("lib.rs");
		if !lib_path.is_file() {
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
		let mutex_cmd = std::sync::Arc::new(std::sync::Mutex::new(cmd));
		let mutex_pallet_impl_path =
			std::sync::Arc::new(std::sync::Mutex::new(self.pallet_impl_path));
		let mutex_lib_path = std::sync::Arc::new(std::sync::Mutex::new(lib_path));
		let mutex_runtime_path = std::sync::Arc::new(std::sync::Mutex::new(runtime_path.clone()));

		for pallet in pallets {
			let mutex_cmd = std::sync::Arc::clone(&mutex_cmd);
			let mutex_pallet_impl_path = std::sync::Arc::clone(&mutex_pallet_impl_path);
			let mutex_lib_path = std::sync::Arc::clone(&mutex_lib_path);
			let mutex_runtime_path = std::sync::Arc::clone(&mutex_runtime_path);

			handles.push(std::thread::spawn(move || -> Result<(), anyhow::Error> {
				let mut cmd = mutex_cmd.lock().map_err(|e| anyhow::Error::msg(format!("{}", e)))?;
				let pallet_impl_path = mutex_pallet_impl_path
					.lock()
					.map_err(|e| anyhow::Error::msg(format!("{}", e)))?;
				let lib_path =
					mutex_lib_path.lock().map_err(|e| anyhow::Error::msg(format!("{}", e)))?;
				let runtime_path =
					mutex_runtime_path.lock().map_err(|e| anyhow::Error::msg(format!("{}", e)))?;

				let mut pallet_computed_path: Option<PathBuf> = None;

				let pallet_impl_path = match pallet_impl_path.as_ref().or_else(|| {
					pallet_computed_path = get_pallet_impl_path(
						&runtime_path,
						&pallet
							.get_crate_name()
							.splitn(2, '-')
							.nth(1)
							.unwrap_or("pallet")
							.to_string(),
					);
					pallet_computed_path.as_ref()
				}) {
					Some(impl_path) => impl_path,
					None => cmd
						.error(
							ErrorKind::InvalidValue,
							"Make sure that the used path correspond to a runtime crate.",
						)
						.exit(),
				};

				// Add the pallet to the crate and to the runtime module
				rust_writer::add_pallet_to_runtime_module(
					&pallet.get_crate_name(),
					&lib_path,
					manifest::types::CrateDependencie::External { version: pallet.get_version() },
				)?;

				// Add the pallet impl block and its related use statements
				rust_writer::add_use_statements(
					&pallet_impl_path,
					pallet.get_impl_needed_use_statements(),
				)?;

				rust_writer::add_pallet_impl_block_to_runtime(
					&pallet.get_crate_name(),
					&pallet_impl_path,
					pallet.get_parameter_types(),
					pallet.get_config_types(),
					pallet.get_config_values(),
					pallet.get_default_config(),
				)?;

				Ok(())
			}));
		}

		for handle in handles {
			let _ = handle.join().expect("Unexpected error");
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
