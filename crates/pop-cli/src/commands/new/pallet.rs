// SPDX-License-Identifier: GPL-3.0

use crate::{cli::{
	traits::Cli as _,
	Cli,
}, utils::helpers::collect_loop_cliclack_inputs};

use crate::pick_options_and_give_name;
use clap::Args;
use cliclack::{outro, outro_cancel, confirm};
use pop_parachains::{
	create_pallet_template, resolve_pallet_path, TemplatePalletConfig, TemplatePalletConfigTypesMetadata, TemplatePalletStorageTypes
};
use std::fs;
use cliclack::{input, select};
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

		let pallet_emit_events = confirm("Is your pallet emitting events? (This will add the RuntimeEvent type to your config trait)").initial_value(true).interact()?;

		Cli.info("Time to specify the types shaping your config trait. Keep adding them until you're done!")?;

		let mut pallet_config_types = Vec::new();

        pick_options_and_give_name!(pallet_config_types, (TemplatePalletConfigTypesMetadata ,"Should the next type be included into the metadata?"));

		Cli.info("Now, let's work on your pallet's storage.")?;

        let mut pallet_storage = Vec::new();
        pick_options_and_give_name!(pallet_storage, (TemplatePalletStorageTypes,"Select a storage type to create an instance of it:"));

        let mut pallet_events = Vec::new();
        if pallet_emit_events{
            Cli.info("Let's add some events now.")?;
            pallet_events = collect_loop_cliclack_inputs("Give a name to your event in order to add it to the template!")?;
        }

        Cli.info(if pallet_emit_events {"And some errors."} else {"Let's add some errors now."})?;
        let pallet_errors = collect_loop_cliclack_inputs("Give a name to your error in order to add it to the template!")?;

        let pallet_default_config = confirm("Would you like to add a derivable default config for your pallet's config trait?").initial_value(true).interact()?;

        let pallet_genesis = confirm("Would you like to add a genesis state for your pallet?").initial_value(true).interact()?;

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
				pallet_emit_events,
				pallet_config_types,
                pallet_storage,
                pallet_events,
                pallet_errors,
                pallet_default_config,
                pallet_genesis
			},
		)?;

		spinner.stop("Generation complete");
		outro(format!("cd into \"{}\" and enjoy hacking! 🚀 Don't forget to complete the todo's! 🙈", &self.name))?;
		Ok(())
	}
}
