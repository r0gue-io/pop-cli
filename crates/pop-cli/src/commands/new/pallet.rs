// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	multiselect_pick,
};

use clap::{Args, Subcommand};
use cliclack::{confirm, input, multiselect, outro, outro_cancel};
use pop_common::{
	add_crate_to_workspace, find_crate_name, find_pallet_runtime_lib_path, find_workspace_toml,
	format_dir, get_pallet_impl_path, manifest::types::CrateDependencie,
	prefix_with_current_dir_if_needed, rust_writer,
};
use pop_parachains::{
	create_pallet_template, TemplatePalletConfig, TemplatePalletConfigCommonTypes,
	TemplatePalletOptions, TemplatePalletStorageTypes,
};
use proc_macro2::Span;
use std::{fs, path::PathBuf};
use strum::{EnumMessage, IntoEnumIterator};
use syn::{Ident, Type};

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
		help = "If your pallet is created in a workspace containing a runtime, Pop-Cli will place the impl blocks for your pallets' Config traits inside a dedicated file under configs directory. Use this argument to point to another path."
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
		let pallet_path = prefix_with_current_dir_if_needed(pallet_path);

		// Determine if the pallet is being created inside a workspace
		let workspace_toml = find_workspace_toml(&pallet_path);

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
		let pallet_crate_name = find_crate_name(&pallet_path.join("Cargo.toml"))?;

		// Check if the pallet has to be included in a runtime and include it if so
		if let Some(runtime_lib_path) = find_pallet_runtime_lib_path(&pallet_path) {
			// If the pallet has been created inside a workspace containing a runtime, add the
			// pallet to that runtime.

			spinner.set_message("Adding the pallet to your runtime if needed...");
			rust_writer::add_pallet_to_runtime_module(
				&pallet_crate_name,
				&runtime_lib_path,
				CrateDependencie::Local { local_crate_path: pallet_path.to_path_buf() },
			)?;

			let pallet_impl_path = if let Some(impl_path) = self.pallet_impl_path {
				impl_path.clone()
			} else {
				get_pallet_impl_path(
					&pallet_path,
					&pallet_crate_name.splitn(2, '-').nth(1).unwrap_or("pallet").to_string(),
				)?
			};

			// Add pallet's impl block
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

			rust_writer::add_pallet_impl_block_to_runtime(
				&pallet_crate_name,
				&pallet_impl_path,
				Vec::new(),
				types,
				values,
				pallet_default_config,
			)?;
		}

		// If the pallet has been created inside a workspace, add it to that workspace
		if let Some(mut workspace_toml) = workspace_toml {
			add_crate_to_workspace(&workspace_toml, &pallet_path)?;
			// Format the whole workspace
			workspace_toml.pop();
			format_dir(&workspace_toml)?;
		} else {
			// Format the pallet dir
			format_dir(&pallet_path)?;
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
