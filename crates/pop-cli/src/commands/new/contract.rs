// SPDX-License-Identifier: GPL-3.0

use clap::{
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
	Args,
};
use cliclack::{clear_screen, confirm, input, intro, outro, outro_cancel, set_theme};
use console::style;
use std::{env::current_dir, fs, path::PathBuf, str::FromStr};

use crate::style::Theme;
use anyhow::Result;
use pop_contracts::{create_smart_contract, Template};
use strum::VariantArray;

#[derive(Args, Clone)]
pub struct NewContractCommand {
	#[arg(help = "Name of the contract")]
	pub(crate) name: Option<String>,
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
	#[arg(
		short = 't',
		long,
		help = "Template to use.",
		value_parser = crate::enum_variants!(Template)
	)]
	pub(crate) template: Option<Template>,
}

impl NewContractCommand {
	pub(crate) async fn execute(&self) -> Result<()> {
		clear_screen()?;
		set_theme(Theme);

		let contract_config = if self.name.is_none() {
			// If user doesn't select the name guide them to generate a contract.
			guide_user_to_generate_contract().await?
		} else {
			self.clone()
		};
		let name = &contract_config
			.name
			.clone()
			.expect("name can not be none as fallback above is interactive input; qed");

		generate_contract_from_template(name, contract_config.path)?;
		Ok(())
	}
}

async fn guide_user_to_generate_contract() -> Result<NewContractCommand> {
	intro(format!("{}: Generate a contract", style(" Pop CLI ").black().on_magenta()))?;
	let name: String = input("Name of your contract?")
		.placeholder("my_contract")
		.default_input("my_contract")
		.interact()?;

	let path: String = input("Where should your project be created?")
		.placeholder("../")
		.default_input("../")
		.interact()?;

	let mut prompt = cliclack::select("Select a template provider: ".to_string());
	for (i, template) in Template::templates().iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(template);
		}
		prompt = prompt.item(template, template.name(), format!("{}", template.description(),));
	}
	let template = prompt.interact()?;

	clear_screen()?;

	Ok(NewContractCommand {
		name: Some(name),
		path: Some(PathBuf::from(path)),
		template: Some(template.clone()),
	})
}

fn generate_contract_from_template(name: &String, path: Option<PathBuf>) -> Result<()> {
	intro(format!(
		"{}: Generating new contract \"{}\"!",
		style(" Pop CLI ").black().on_magenta(),
		name,
	))?;
	let contract_path =
		if let Some(ref path) = path { path.join(name) } else { current_dir()?.join(name) };
	if contract_path.exists() {
		if !confirm(format!(
			"\"{}\" directory already exists. Would you like to remove it?",
			contract_path.display()
		))
		.interact()?
		{
			outro_cancel(format!(
				"Cannot generate contract until \"{}\" directory is removed.",
				contract_path.display()
			))?;
			return Ok(());
		}
		fs::remove_dir_all(contract_path.as_path())?;
	}
	fs::create_dir_all(contract_path.as_path())?;
	let spinner = cliclack::spinner();
	spinner.start("Generating contract...");
	create_smart_contract(&name, contract_path.as_path())?;

	spinner.stop("Smart contract created!");
	outro(format!("cd into \"{}\" and enjoy hacking! 🚀", contract_path.display()))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[tokio::test]
	async fn test_new_contract_command_execute_success() -> Result<()> {
		let temp_contract_dir = tempfile::tempdir().expect("Could not create temp dir");
		let command = NewContractCommand {
			name: Some("test_contract".to_string()),
			path: Some(PathBuf::from(temp_contract_dir.path())),
		};
		command.execute().await?;
		Ok(())
	}
}
