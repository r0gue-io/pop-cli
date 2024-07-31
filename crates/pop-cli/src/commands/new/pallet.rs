// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	utils::helpers::{collect_loop_cliclack_inputs, valid_ident},
};

use crate::{multiselect_pick, pick_options_and_give_name};
use clap::{Args, Subcommand};
use cliclack::{confirm, input, multiselect, outro, outro_cancel, select};
use pop_parachains::{
	create_pallet_template, resolve_pallet_path, TemplatePalletConfig,
	TemplatePalletConfigCommonTypes, TemplatePalletConfigTypesDefault,
	TemplatePalletConfigTypesMetadata, TemplatePalletStorageTypes,
};
use std::fs;
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
	#[arg(short, long, help = "Add types to your config trait from the CLI.")]
	pub(crate) config_trait: bool,
	#[arg(short, long, help = "Use a default configuration for your config trait.")]
	pub(crate) default_config: bool,
	#[arg(short, long, help = "Add storage items to your pallet from the CLI.")]
	pub(crate) storage: bool,
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
		let mut pallet_config_types = Vec::new();
		let mut pallet_storage = Vec::new();
		let mut pallet_genesis = false;
        let mut pallet_custom_origin = false;
		let mut pallet_custom_origin_variants = Vec::new();

		if let Some(Mode::Advanced(advanced_mode_args)) = &self.mode {
			pallet_default_config = advanced_mode_args.default_config;
			pallet_genesis = advanced_mode_args.genesis_config;
            pallet_custom_origin = advanced_mode_args.custom_origin;

			if advanced_mode_args.config_trait {
				Cli.info("Generate the pallet's config trait.")?;

				pallet_common_types = multiselect_pick!(TemplatePalletConfigCommonTypes, "Are you interested in adding one of these types and their usual configuration to your pallet?");

				Cli.info("If you want to add some other config types, this is the moment. Keep adding them until you're done!")?;

				// Depending on the user's selection, the cli should offer to choose wheter the type is
				// included in the default config or not.
				if advanced_mode_args.default_config {
					pallet_config_types = pick_options_and_give_name!(
                        (TemplatePalletConfigTypesMetadata ,"You're adding a new type to your pallet's Config trait! Should it be included in the metadata?"), 
                        (TemplatePalletConfigTypesDefault, "Should it be included in the default configuration?")
                    );
				} else {
					// As the vec must contain as well info related to the default config, we map everything
					// to have the Default variant. The type annotations happen when the type's no default,
					// so this variant ensures there's no type annotation related to default, even though
					// there's no default config and then the types cannot be default
					pallet_config_types = pick_options_and_give_name!(
                        (TemplatePalletConfigTypesMetadata ,"You're adding a new type to your pallet's Config trait! Should it be included in the metadata?")
                    )
                        .into_iter()
                        .map(|(to_metadata, config_type)| (to_metadata, TemplatePalletConfigTypesDefault::Default, config_type))
                        .collect::<Vec<(TemplatePalletConfigTypesMetadata, TemplatePalletConfigTypesDefault, String)>>();
				}
			}

			if advanced_mode_args.storage {
				Cli.info("Generate the pallet's storage.")?;

				pallet_storage = pick_options_and_give_name!(
                    (TemplatePalletStorageTypes,"You're adding a new storage item to your pallet! Select a storage type to create an instance of it:")
                );
			}

			if pallet_custom_origin {
				Cli.info("Generate the pallet's custom origin")?;

				pallet_custom_origin_variants =
					collect_loop_cliclack_inputs("Add a variant name.")?;
			}
		};

		let target = resolve_pallet_path(self.path.clone())?;
		let pallet_name = self.name.clone();
		let pallet_path = target.join(pallet_name.clone());
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
			fs::remove_dir_all(pallet_path)?;
		}
		let spinner = cliclack::spinner();
		spinner.start("Generating pallet...");
		create_pallet_template(
			self.path.clone(),
			TemplatePalletConfig {
				name: self.name.clone(),
				authors: self.authors.clone().expect("default values"),
				description: self.description.clone().expect("default values"),
				pallet_advanced_mode: self.mode.is_some(),
				pallet_default_config,
				pallet_common_types,
				pallet_config_types,
				pallet_storage,
				pallet_genesis,
                pallet_custom_origin,
				pallet_custom_origin_variants,
			},
		)?;

		spinner.stop("Generation complete");
		outro(format!("cd into \"{}\" and enjoy hacking! 🚀", &self.name))?;
		Ok(())
	}
}
