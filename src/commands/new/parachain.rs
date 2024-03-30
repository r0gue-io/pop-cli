use crate::{
	engines::parachain_engine::{instantiate_template_dir, Config},
	helpers::git_init,
	style::{style, Theme},
};
use clap::{Args, Parser};
use std::{fs, path::Path};
use strum_macros::{Display, EnumString};

use cliclack::{clear_screen, confirm, intro, outro, outro_cancel, set_theme,log};

#[derive(Clone, Parser, Debug, Display, EnumString, PartialEq)]
pub enum Template {
	#[strum(serialize = "Contracts Node Template", serialize = "cpt")]
	Contracts,
	#[strum(serialize = "Frontier Parachain Template", serialize = "fpt")]
	FPT,
	#[strum(serialize = "Base Parachain Template", serialize = "base")]
	Base,
}

#[derive(Args)]
pub struct NewParachainCommand {
	#[arg(help = "Name of the project. Also works as a directory path for your project")]
	pub(crate) name: String,
	#[arg(
		help = "Template to use; Options are 'cpt', 'fpt'. Leave empty for default parachain template"
	)]
	#[arg(default_value = "base")]
	pub(crate) template: Template,
	#[arg(long, short, help = "Token Symbol", default_value = "UNIT")]
	pub(crate) symbol: Option<String>,
	#[arg(long, short, help = "Token Decimals", default_value = "12")]
	pub(crate) decimals: Option<u8>,
	#[arg(
		long = "endowment",
		short,
		help = "Token Endowment for dev accounts",
		default_value = "1u64 << 60"
	)]
	pub(crate) initial_endowment: Option<String>,
}

impl NewParachainCommand {
	pub(crate) fn execute(&self) -> anyhow::Result<()> {
		clear_screen()?;
		intro(format!(
			"{}: Generating \"{}\" using {}!",
			style(" Pop CLI ").black().on_magenta(),
			&self.name,
			&self.template
		))?;
		set_theme(Theme);
		let destination_path = Path::new(&self.name);
		if destination_path.exists() {
			if !confirm(format!(
				"\"{}\" directory already exists. Would you like to remove it?",
				destination_path.display()
			))
			.interact()?
			{
				outro_cancel(format!(
					"Cannot generate parachain until \"{}\" directory is removed.",
					destination_path.display()
				))?;
				return Ok(())
			}
			fs::remove_dir_all(destination_path)?;
		}
		let mut spinner = cliclack::spinner();
		spinner.start("Generating parachain...");
		let tag = instantiate_template_dir(
			&self.template,
			destination_path,
			Config {
				symbol: self.symbol.clone().expect("default values"),
				decimals: self.decimals.clone().expect("default values"),
				initial_endowment: self.initial_endowment.clone().expect("default values"),
			},
		)?;
		if let Err(err) = git_init(destination_path, "initialized parachain") {
			if err.class() == git2::ErrorClass::Config && err.code() == git2::ErrorCode::NotFound {
				outro_cancel("git signature could not be found. Please configure your git config with your name and email")?;
			}
		}
		spinner.stop("Generation complete");
		log::info(format!("Version: {}", tag))?;
		outro(format!("cd into \"{}\" and enjoy hacking! 🚀", &self.name))?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {

	use git2::Repository;

	use super::*;
	use std::fs;

	#[test]
	fn test_new_parachain_command_execute() -> anyhow::Result<()> {
		let command = NewParachainCommand {
			name: "test_parachain".to_string(),
			template: Template::Base,
			symbol: Some("UNIT".to_string()),
			decimals: Some(12),
			initial_endowment: Some("1u64 << 60".to_string()),
		};
		let result = command.execute();
		assert!(result.is_ok());

		// check for git_init
		let repo = Repository::open(Path::new(&command.name))?;
		let reflog = repo.reflog("HEAD")?;
		assert_eq!(reflog.len(), 1);

		// Clean up
		if let Err(err) = fs::remove_dir_all("test_parachain") {
			eprintln!("Failed to delete directory: {}", err);
		}
		Ok(())
	}
}
