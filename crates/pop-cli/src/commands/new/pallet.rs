// SPDX-License-Identifier: GPL-3.0

use crate::style::Theme;
use clap::Args;
use cliclack::{clear_screen, confirm, intro, outro, outro_cancel, set_theme};
use console::style;
use pop_parachains::{create_pallet_template, resolve_pallet_path, TemplatePalletConfig};
use std::fs;

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
		clear_screen()?;
		intro(format!(
			"{}: Generating new pallet \"{}\"!",
			style(" Pop CLI ").black().on_magenta(),
			&self.name,
		))?;
		set_theme(Theme);
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
			},
		)?;

		spinner.stop("Generation complete");
		outro(format!("cd into \"{}\" and enjoy hacking! 🚀", &self.name))?;
		Ok(())
	}
}
