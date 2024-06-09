// SPDX-License-Identifier: GPL-3.0

use clap::{
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
	Args,
};
use cliclack::{clear_screen, confirm, input, intro, log::success, outro, outro_cancel, set_theme};
use console::style;
use std::{env::current_dir, fs, path::PathBuf, str::FromStr};

use crate::style::Theme;
use anyhow::Result;
use pop_contracts::{create_smart_contract, is_valid_contract_name, Template};
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

		is_valid_contract_name(name)?;

		let template = match &contract_config.template {
			Some(template) => template.clone(),
			None => Template::Standard, // Default template
		};

		generate_contract_from_template(name, contract_config.path, &template)?;
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

fn generate_contract_from_template(
	name: &String,
	path: Option<PathBuf>,
	template: &Template,
) -> Result<()> {
	intro(format!(
		"{}: Generating \"{}\" using a {} template!",
		style(" Pop CLI ").black().on_magenta(),
		name,
		template.name(),
	))?;
	let contract_path = check_destination_path(path, name)?;

	fs::create_dir_all(contract_path.as_path())?;
	let spinner = cliclack::spinner();
	spinner.start("Generating contract...");

	create_smart_contract(&name, contract_path.as_path(), template)?;

	spinner.clear();

	// replace spinner with success
	console::Term::stderr().clear_last_lines(2)?;
	success("Generation complete")?;

	// add next steps
	let mut next_steps = vec![
		format!("cd into {:?} and enjoy hacking! 🚀", contract_path.display()),
		"Use `pop build contract` to build your contract.".into(),
	];
	next_steps.push(format!("Use `pop up contract` to deploy your contract on a live network."));
	let next_steps: Vec<_> = next_steps
		.iter()
		.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
		.collect();
	success(format!("Next Steps:\n{}", next_steps.join("\n")))?;

	outro(format!(
		"Need help? Learn more at {}\n",
		style("https://learn.onpop.io/v/cli").magenta().underlined()
	))?;
	Ok(())
}

fn check_destination_path(path: Option<PathBuf>, name: &String) -> Result<PathBuf> {
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
			return Err(anyhow::anyhow!(format!(
				"\"{}\" directory already exists.",
				contract_path.display()
			)));
		}
		fs::remove_dir_all(contract_path.as_path())?;
	}
	Ok(contract_path)
}

#[cfg(test)]
mod tests {
	use crate::{
		commands::new::{NewArgs, NewCommands::Contract},
		Cli,
		Commands::New,
	};
	use anyhow::Result;
	use clap::Parser;
	use tempfile::tempdir;

	#[tokio::test]
	async fn test_new_contract_command_execute_with_defaults_executes() -> Result<()> {
		let dir = tempdir()?;
		let dir_path = dir.path().display().to_string();
		let cli = Cli::parse_from(["pop", "new", "contract", "test_contract", "-p", &dir_path]);

		let New(NewArgs { command: Contract(command) }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		command.execute().await?;
		Ok(())
	}

	#[tokio::test]
	async fn test_new_contract_template_command_execute() -> Result<()> {
		let dir = tempdir()?;
		let dir_path = dir.path().display().to_string();
		let cli = Cli::parse_from([
			"pop",
			"new",
			"contract",
			"test_contract",
			"-p",
			&dir_path,
			"-t",
			"erc20",
		]);

		let New(NewArgs { command: Contract(command) }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		command.execute().await?;
		Ok(())
	}
}
