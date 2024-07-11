// SPDX-License-Identifier: GPL-3.0

use crate::style::Theme;
use anyhow::Result;
use clap::{
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
	Args,
};
use cliclack::{clear_screen, confirm, input, intro, log::success, outro, outro_cancel, set_theme};
use console::style;
use pop_common::{
	enum_variants,
	templates::{Template, Type},
};
use pop_contracts::{create_smart_contract, is_valid_contract_name, Contract, ContractType};
use std::{env::current_dir, fs, path::PathBuf, str::FromStr};
use strum::VariantArray;

#[derive(Args, Clone)]
pub struct NewContractCommand {
	#[arg(help = "Name of the contract")]
	pub(crate) name: Option<String>,
	#[arg(
		default_value = ContractType::Examples.as_ref(),
		short = 'c',
		long,
		help = "Contract type.",
		value_parser = enum_variants!(ContractType)
	)]
	pub(crate) contract_type: Option<ContractType>,
	#[arg(short = 'p', long, help = "Path for the contract project, [default: current directory]")]
	pub(crate) path: Option<PathBuf>,
	#[arg(
		short = 't',
		long,
		help = "Template to use.",
		value_parser = enum_variants!(Contract)
	)]
	pub(crate) template: Option<Contract>,
}

impl NewContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> anyhow::Result<()> {
		clear_screen()?;
		set_theme(Theme);
		let contract_config = if self.name.is_none() {
			// If the user doesn't provide a name, guide them in generating a contract.
			guide_user_to_generate_contract().await?
		} else {
			self.clone()
		};
		let name = &contract_config
			.name
			.clone()
			.expect("name can not be none as fallback above is interactive input; qed");
		is_valid_contract_name(name)?;
		let contract_type = &contract_config.contract_type.clone().unwrap_or_default();
		let template = match &contract_config.template {
			Some(template) => template.clone(),
			None => contract_type.default_template().expect("contract types have defaults; qed."), // Default contract type
		};

		is_template_supported(contract_type, &template)?;

		generate_contract_from_template(name, contract_config.path, &template)?;
		Ok(())
	}
}

fn is_template_supported(contract_type: &ContractType, template: &Contract) -> Result<()> {
	if !contract_type.provides(template) {
		return Err(anyhow::anyhow!(format!(
			"The contract type \"{:?}\" doesn't support the {:?} template.",
			contract_type, template
		)));
	};
	return Ok(());
}

async fn guide_user_to_generate_contract() -> anyhow::Result<NewContractCommand> {
	intro(format!("{}: Generate a contract", style(" Pop CLI ").black().on_magenta()))?;
	let name: String = input("Name of your contract?")
		.placeholder("my_contract")
		.default_input("my_contract")
		.interact()?;
	let path: String = input("Where should your project be created?")
		.placeholder("./")
		.default_input("./")
		.interact()?;

	let mut contract_type_prompt = cliclack::select("Select a template type: ".to_string());
	for (i, contract_type) in ContractType::types().iter().enumerate() {
		if i == 0 {
			contract_type_prompt = contract_type_prompt.initial_value(contract_type);
		}
		contract_type_prompt = contract_type_prompt.item(
			contract_type,
			contract_type.name(),
			format!(
				"{} {} available option(s)",
				contract_type.description(),
				contract_type.templates().len(),
			),
		);
	}
	let contract_type = contract_type_prompt.interact()?;

	let template = display_select_options(contract_type)?;

	clear_screen()?;
	Ok(NewContractCommand {
		name: Some(name),
		path: Some(PathBuf::from(path)),
		contract_type: Some(contract_type.clone()),
		template: Some(template.clone()),
	})
}

fn display_select_options(contract_type: &ContractType) -> Result<&Contract> {
	let mut prompt = cliclack::select("Select the contract:".to_string());
	for (i, template) in contract_type.templates().into_iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(template);
		}
		prompt = prompt.item(template, template.name(), template.description());
	}
	Ok(prompt.interact()?)
}

fn generate_contract_from_template(
	name: &String,
	path: Option<PathBuf>,
	template: &Contract,
) -> anyhow::Result<()> {
	intro(format!(
		"{}: Generating \"{}\" using {}!",
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
	// Replace spinner with success.
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

fn check_destination_path(path: Option<PathBuf>, name: &String) -> anyhow::Result<PathBuf> {
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
		commands::new::{Command::Contract, NewArgs},
		Cli,
		Command::New,
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
			"-c",
			"erc",
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
