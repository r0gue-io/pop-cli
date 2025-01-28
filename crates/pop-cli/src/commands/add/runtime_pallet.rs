// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};
use clap::{error::ErrorKind, Args, Command};
use cliclack::multiselect;
use pop_common::{
	find_pallet_runtime_impl_path, find_workspace_toml, format_dir,
	manifest::types::CrateDependencie, prefix_with_current_dir_if_needed, rust_writer,
};
use std::{env::current_dir, path::PathBuf};
use strum::{EnumMessage, IntoEnumIterator};

mod common_pallets;

#[derive(Args, Debug, Clone)]
pub struct AddPalletCommand {
	#[arg(short, long, help = "Specify the path to the runtime crate.")]
	pub(crate) runtime_path: Option<PathBuf>,
	#[arg(short, long, value_enum, num_args(1..), required = false, help = "The pallets you want to include to your runtime.")]
	pub(crate) pallets: Vec<common_pallets::CommonPallets>,
	#[arg(
		long,
		help = "Pop-Cli will place the impl blocks for your pallets' Config traits inside configs/mod.rs or lib.rs in the runtime crate by default. If you want to place them in another path, use this option to specify it."
	)]
	pub(crate) runtime_impl_path: Option<PathBuf>,
}

impl AddPalletCommand {
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Add a new pallet to your runtime")?;
		let runtime_path = if let Some(path) = &self.runtime_path {
			prefix_with_current_dir_if_needed(path.to_path_buf())
		} else {
			let working_dir = current_dir().expect("Cannot modify your working directory");
			// Give the chance to use the command either from a workspace containing a runtime or
			// from a runtime crate if path not specified
			if working_dir.join("runtime").exists() {
				prefix_with_current_dir_if_needed(working_dir.join("runtime"))
			} else {
				prefix_with_current_dir_if_needed(working_dir)
			}
		};
		let mut cmd = Command::new("");
		let src = runtime_path.join("src");
		let lib_path = src.join("lib.rs");
		if !lib_path.is_file() {
			cmd.error(
				ErrorKind::InvalidValue,
				"Make sure to run this command either in a workspace containing a runtime crate/a runtime crate or to specify the path to the runtime crate using -r.",
			)
			.exit();
		}

		let runtime_impl_path =
			match self.runtime_impl_path.or_else(|| find_pallet_runtime_impl_path(&lib_path)) {
				Some(runtime_impl_path) => runtime_impl_path,
				None => cmd
					.error(
						ErrorKind::InvalidValue,
						"Make sure that the used path correspond to a runtime crate.",
					)
					.exit(),
			};

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

		for pallet in pallets {
			// Add the pallet to the crate and to the runtime module
			rust_writer::add_pallet_to_runtime_module(
				&pallet.get_crate_name(),
				&lib_path,
				CrateDependencie::External { version: pallet.get_version() },
			)?;
			// Add the pallet impl block
			rust_writer::add_pallet_impl_block_to_runtime(
				&pallet.get_crate_name(),
				&runtime_impl_path,
				pallet.get_parameter_types(),
				pallet.get_config_types(),
				pallet.get_config_values(),
				pallet.get_default_config(),
			)?;
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
