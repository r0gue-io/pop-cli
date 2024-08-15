// SPDX-License-Identifier: GPL-3.0

use crate::cli::{traits::Cli as _, Cli};

use clap::{Args, Subcommand};
use cliclack::{confirm, multiselect, outro, outro_cancel};
use pop_common::{
	manifest::{add_crate_to_workspace, find_workspace_toml},
	multiselect_pick,
};
use pop_parachains::{
	create_pallet_template, resolve_pallet_path, TemplatePalletConfig,
	TemplatePalletConfigCommonTypes, TemplatePalletStorageTypes,
};
use std::{fs, process::Command};
use strum::{EnumMessage, IntoEnumIterator};

#[derive(Args)]
pub struct NewPalletCommand {
	#[command(subcommand)]
	pub(crate) mode: Option<Mode>,
	#[arg(help = "Name of the pallet", default_value = "pallet-template")]
	pub(crate) name: String,
	#[arg(short, long, help = "Name of authors", default_value = "Anonymous")]
	pub(crate) authors: Option<String>,
	#[arg(short, long, help = "Pallet description", default_value = "Frame Pallet")]
	pub(crate) description: Option<String>,
	#[arg(short = 'p', long, help = "Path to the pallet, [default: current directory]")]
	pub(crate) path: Option<String>,
}

#[derive(Subcommand)]
pub enum Mode {
	/// Using the advanced mode will unlock all the POP CLI potential. You'll be able to fully customize your pallet template!. Don't use this mode unless you exactly know what you want for your pallet
	Advanced(AdvancedMode),
}

#[derive(Args)]
pub struct AdvancedMode {
	#[arg(short, long, value_enum, help = "Add common types to your config trait from the CLI.")]
	pub(crate) config_common_types: Option<Vec<TemplatePalletConfigCommonTypes>>,
	#[arg(short, long, help = "Use a default configuration for your config trait.")]
	pub(crate) default_config: bool,
	#[arg(short, long, value_enum, help = "Add storage items to your pallet from the CLI.")]
	pub(crate) storage: Option<Vec<TemplatePalletStorageTypes>>,
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
			pallet_default_config = advanced_mode_args.default_config;
			pallet_genesis = advanced_mode_args.genesis_config;
			pallet_custom_origin = advanced_mode_args.custom_origin;

			match &advanced_mode_args.config_common_types {
				Some(selected_options) => pallet_common_types = selected_options.clone(),
				None => {
					Cli.info("Generate the pallet's config trait.")?;

					pallet_common_types = multiselect_pick!(TemplatePalletConfigCommonTypes, "Are you interested in adding one of these types and their usual configuration to your pallet?");
				},
			}

			match &advanced_mode_args.storage {
				Some(selected_options) => pallet_storage = selected_options.clone(),
				None => {
					Cli.info("Generate the pallet's storage.")?;

					pallet_storage = multiselect_pick!(
						TemplatePalletStorageTypes,
						"Are you interested in adding some of those storage items to your pallet?"
					);
				},
			}
		};

		let target = resolve_pallet_path(self.path.clone())?;

		let pallet_name = self.name.clone();
		let pallet_path = target.join(pallet_name.clone());
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
			target,
			TemplatePalletConfig {
				name: self.name.clone(),
				authors: self.authors.clone().expect("default values"),
				description: self.description.clone().expect("default values"),
				pallet_in_workspace: workspace_toml.is_some(),
				pallet_advanced_mode: self.mode.is_some(),
				pallet_default_config,
				pallet_common_types,
				pallet_storage,
				pallet_genesis,
				pallet_custom_origin,
			},
		)?;

		// If the pallet has been created inside a workspace, add it to that workspace
		if let Some(workspace_toml) = workspace_toml {
			add_crate_to_workspace(&workspace_toml, &pallet_path)?;
		}

		// Format the dir. If this fails we do nothing, it's not a major failure
		let _ = Command::new("cargo").arg("fmt").current_dir(pallet_path).output();

		spinner.stop("Generation complete");
		outro(format!("cd into \"{}\" and enjoy hacking! 🚀", &self.name))?;
		Ok(())
	}
}
