// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	utils::helpers::collect_loop_cliclack_inputs,
};

use crate::{multiselect_pick, pick_options_and_give_name};
use clap::Args;
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
	#[arg(help = "Name of the pallet", default_value = "pallet-template")]
	pub(crate) name: String,
	#[arg(short, long, help = "Name of authors", default_value = "Anonymous")]
	pub(crate) authors: Option<String>,
	#[arg(short, long, help = "Pallet description", default_value = "Frame Pallet")]
	pub(crate) description: Option<String>,
	#[arg(short = 'p', long, help = "Path to the pallet, [default: current directory]")]
	pub(crate) path: Option<String>,
}

impl NewPalletCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		Cli.intro("Let's create a new pallet!")?;

		Cli.info("First, we're defining the pallet's config trait.")?;

		let pallet_default_config = confirm("Would you like to add a derivable default configuration for your pallet's config trait?").initial_value(true).interact()?;

		let pallet_common_types = multiselect_pick!(TemplatePalletConfigCommonTypes, "Are you interested in adding one of these types and their usual configuration to your pallet?");

		Cli.info("If you want to add some other config types, this is the moment. Keep adding them until you're done!")?;

		let pallet_config_types;
		// Depending on the user's selection, the cli should offer to choose wheter the type is
		// included in the default config or not.
		if pallet_default_config {
			pallet_config_types = pick_options_and_give_name!(
                (TemplatePalletConfigTypesMetadata ,"Your adding a new type to your pallet's Config trait! Should it be included into the metadata?"), 
                (TemplatePalletConfigTypesDefault, "Should it be included in the default configuration?")
            );
		} else {
			// As the vec must contain as well info related to the default config, we map everything
			// to have the Default variant. The type annotations happen when the type's no default,
			// so this variant ensures there's no type annotation related to default, even though
			// there's no default config and then the types cannot be default
			pallet_config_types = pick_options_and_give_name!(
                (TemplatePalletConfigTypesMetadata ,"You're adding a new type to your pallet's Config trait! Should it be included into the metadata?")
            )
                .into_iter()
                .map(|(to_metadata, config_type)| (to_metadata, TemplatePalletConfigTypesDefault::Default, config_type))
                .collect::<Vec<(TemplatePalletConfigTypesMetadata, TemplatePalletConfigTypesDefault, String)>>();
		}

		Cli.info("Now, let's work on your pallet's storage.")?;

		let pallet_storage = pick_options_and_give_name!(
            (TemplatePalletStorageTypes,"You're adding a new storage item to your pallet! Select a storage type to create an instance of it:")
        );

		let pallet_genesis = confirm("Would you like to add a genesis state for your pallet?")
			.initial_value(true)
			.interact()?;

		let mut pallet_custom_internal_origin_variants = Vec::new();
		if pallet_common_types.contains(&TemplatePalletConfigCommonTypes::RuntimeOrigin) && confirm("Would you like to add a custom internal origin? If yes, you'll be asked to add the variants of the Origin enum.").initial_value(true).interact()?{
            pallet_custom_internal_origin_variants = collect_loop_cliclack_inputs("Add a variant name.")?;
        }

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
				pallet_default_config,
				pallet_common_types,
				pallet_config_types,
				pallet_storage,
				pallet_genesis,
				pallet_custom_internal_origin_variants,
			},
		)?;

		spinner.stop("Generation complete");
		outro(format!(
			"cd into \"{}\" and enjoy hacking! 🚀 Don't forget to complete the todo's! 🙈",
			&self.name
		))?;
		Ok(())
	}
}
