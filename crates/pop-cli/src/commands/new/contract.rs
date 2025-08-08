// SPDX-License-Identifier: GPL-3.0

use crate::cli::{
	traits::{Cli as _, Confirm as _},
	Cli,
};
use pop_common::manifest::{add_crate_to_workspace, find_workspace_toml};

use anyhow::Result;
use clap::{
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
	Args,
};
use cliclack::input;
use console::style;
use pop_common::{
	enum_variants, get_project_name_from_path,
	templates::{Template, Type},
};
use pop_contracts::{create_smart_contract, is_valid_contract_name, Contract, ContractType};
use std::{
	fs,
	path::{Path, PathBuf},
	str::FromStr,
};
use strum::VariantArray;

#[derive(Args, Clone)]
#[cfg_attr(test, derive(Default))]
pub struct NewContractCommand {
	/// The name of the contract.
	pub(crate) name: Option<String>,
	/// The type of contract.
	#[arg(
		default_value = ContractType::Examples.as_ref(),
		short,
		long,
		value_parser = enum_variants!(ContractType)
	)]
	pub(crate) contract_type: Option<ContractType>,
	/// The template to use.
	#[arg(short, long, value_parser = enum_variants!(Contract))]
	pub(crate) template: Option<Contract>,
}

impl NewContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<Contract> {
		// If the user doesn't provide a name, guide them in generating a contract.
		let contract_config = if self.name.is_none() {
			guide_user_to_generate_contract().await?
		} else {
			self.clone()
		};

		let path_project = &contract_config
			.name
			.clone()
			.expect("name can not be none as fallback above is interactive input; qed");
		let path = Path::new(path_project);
		let name = get_project_name_from_path(path, "my_contract");

		// Validate contract name.
		if let Err(e) = is_valid_contract_name(name) {
			Cli.outro_cancel(e)?;
			return Ok(Contract::Standard);
		}

		let contract_type = &contract_config.contract_type.clone().unwrap_or_default();
		let template = match &contract_config.template {
			Some(template) => template.clone(),
			None => contract_type.default_template().expect("contract types have defaults; qed."), /* Default contract type */
		};

		is_template_supported(contract_type, &template)?;
		generate_contract_from_template(name, path, &template)?;

		// If the contract is part of a workspace, add it to that workspace
		if let Some(workspace_toml) = find_workspace_toml(path) {
			add_crate_to_workspace(&workspace_toml, path)?;
		}

		Ok(template)
	}
}

/// Determines whether the specified template is supported by the type.
fn is_template_supported(contract_type: &ContractType, template: &Contract) -> Result<()> {
	if !contract_type.provides(template) {
		return Err(anyhow::anyhow!(format!(
			"The contract type \"{:?}\" doesn't support the {:?} template.",
			contract_type, template
		)));
	};
	Ok(())
}

/// Guide the user to generate a contract from available templates.
async fn guide_user_to_generate_contract() -> Result<NewContractCommand> {
	Cli.intro("Generate a contract")?;

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

	// Prompt for location.
	let name: String = input("Where should your project be created?")
		.placeholder("./my_contract")
		.default_input("./my_contract")
		.interact()?;

	Ok(NewContractCommand {
		name: Some(name),
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
	name: &str,
	path: &Path,
	template: &Contract,
) -> anyhow::Result<()> {
	Cli.intro(format!("Generating \"{}\" using {}!", name, template.name(),))?;

	let contract_path = check_destination_path(path)?;
	fs::create_dir_all(contract_path.as_path())?;
	let spinner = cliclack::spinner();
	spinner.start("Generating contract...");
	create_smart_contract(name, contract_path.as_path(), template)?;
	spinner.clear();
	// Replace spinner with success.
	console::Term::stderr().clear_last_lines(2)?;
	Cli.success("Generation complete")?;

	// warn about audit status and licensing
	let repository = template.repository_url().ok().map(|url|
		style(format!("\nPlease consult the source repository at {url} to assess production suitability and licensing restrictions.")).dim()
	);
	Cli.warning(format!("NOTE: the resulting contract is not guaranteed to be audited or reviewed for security vulnerabilities.{}",
					repository.unwrap_or_else(|| style("".to_string()))))?;

	// add next steps
	let mut next_steps = vec![
		format!("cd into {:?} and enjoy hacking! 🚀", contract_path.display()),
		"Use `pop build` to build your contract.".into(),
	];
	next_steps.push("Use `pop up contract` to deploy your contract to a live network.".to_string());
	let next_steps: Vec<_> = next_steps
		.iter()
		.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
		.collect();
	Cli.success(format!("Next Steps:\n{}", next_steps.join("\n")))?;

	Cli.outro(format!(
		"Need help? Learn more at {}\n",
		style("https://learn.onpop.io").magenta().underlined()
	))?;
	Ok(())
}

fn check_destination_path(contract_path: &Path) -> anyhow::Result<PathBuf> {
	if contract_path.exists() {
		if !Cli
			.confirm(format!(
				"\"{}\" directory already exists. Would you like to remove it?",
				contract_path.display()
			))
			.interact()?
		{
			Cli.outro_cancel(format!(
				"Cannot generate contract until \"{}\" directory is removed.",
				contract_path.display()
			))?;
			return Err(anyhow::anyhow!(format!(
				"\"{}\" directory already exists.",
				contract_path.display()
			)));
		}
		fs::remove_dir_all(contract_path)?;
	}
	Ok(contract_path.to_path_buf())
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
		let dir_path = format!("{}/test_contract", dir.path().display().to_string());
		let cli = Cli::parse_from(["pop", "new", "contract", &dir_path]);

		let New(NewArgs { command: Some(Contract(command)) }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		command.execute().await?;
		Ok(())
	}
}
