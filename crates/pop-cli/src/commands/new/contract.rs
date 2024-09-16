// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::*},
	common::helpers::check_destination_path,
};
use pop_common::manifest::{add_crate_to_workspace, find_workspace_toml};

use anyhow::Result;
use clap::{
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
	Args,
};
use console::style;
use pop_common::{
	enum_variants, get_project_name_from_path,
	templates::{Template, Type},
};
use pop_contracts::{create_smart_contract, is_valid_contract_name, Contract, ContractType};
use std::{fs, path::Path, str::FromStr};
use strum::VariantArray;

#[derive(Args, Clone)]
pub struct NewContractCommand {
	/// The name of the contract.
	pub(crate) name: Option<String>,
	/// The type of contract.
	#[arg(
		default_value = ContractType::Examples.as_ref(),
		short = 'c',
		long,
		value_parser = enum_variants!(ContractType)
	)]
	pub(crate) contract_type: Option<ContractType>,
	/// The template to use.
	#[arg(
		short = 't',
		long,
		value_parser = enum_variants!(Contract)
	)]
	pub(crate) template: Option<Contract>,
}

impl NewContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<()> {
		// If the user doesn't provide a name, guide them in generating a contract.
		let contract_config = if self.name.is_none() {
			guide_user_to_generate_contract(&mut cli::Cli).await?
		} else {
			self.clone()
		};

		let path_project = &contract_config
			.name
			.clone()
			.expect("name can not be none as fallback above is interactive input; qed");
		let path = Path::new(path_project);
		let name = get_project_name_from_path(path, "my_contract");

		// If contract name is invalid finish.
		if !is_valid_name(name, &mut cli::Cli)? {
			return Ok(());
		}

		let contract_type = &contract_config.contract_type.clone().unwrap_or_default();
		let template = match &contract_config.template {
			Some(template) => template.clone(),
			None => contract_type.default_template().expect("contract types have defaults; qed."), /* Default contract type */
		};

		is_template_supported(contract_type, &template)?;
		generate_contract_from_template(name, &path, &template, &mut cli::Cli)?;

		// If the contract is part of a workspace, add it to that workspace
		if let Some(workspace_toml) = find_workspace_toml(&path) {
			add_crate_to_workspace(&workspace_toml, &path)?;
		}

		Ok(())
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
	return Ok(());
}

/// Determines whether the specified name is a valid contract name.
fn is_valid_name(name: &str, cli: &mut impl cli::traits::Cli) -> Result<bool> {
	if let Err(e) = is_valid_contract_name(name) {
		cli.outro_cancel(e)?;
		return Ok(false);
	}
	return Ok(true);
}

/// Guide the user to generate a contract from available templates.
async fn guide_user_to_generate_contract(
	cli: &mut impl cli::traits::Cli,
) -> Result<NewContractCommand> {
	cli.intro("Generate a contract")?;

	let contract_type = {
		let mut contract_type_prompt = cli.select("Select a template type:".to_string());
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
		contract_type_prompt.interact()?
	};
	let template = {
		let mut prompt = cli.select("Select the contract:".to_string());
		for (i, template) in contract_type.templates().into_iter().enumerate() {
			if i == 0 {
				prompt = prompt.initial_value(template);
			}
			prompt = prompt.item(template, template.name(), template.description());
		}
		prompt.interact()?
	};

	// Prompt for location.
	let name: String = cli
		.input("Where should your project be created?")
		.placeholder("./my_contract")
		.default_input("./my_contract")
		.interact()?;

	Ok(NewContractCommand {
		name: Some(name),
		contract_type: Some(contract_type.clone()),
		template: Some(template.clone()),
	})
}

fn generate_contract_from_template(
	name: &str,
	path: &Path,
	template: &Contract,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	cli.intro(format!("Generating \"{}\" using {}!", name, template.name(),))?;

	let contract_path = check_destination_path(path, cli)?;
	fs::create_dir_all(contract_path.as_path())?;
	let spinner = cliclack::spinner();
	spinner.start("Generating contract...");
	create_smart_contract(&name, contract_path.as_path(), template)?;
	spinner.clear();
	// Replace spinner with success.
	console::Term::stderr().clear_last_lines(2)?;
	cli.success("Generation complete")?;

	// warn about audit status and licensing
	let repository = template.repository_url().ok().map(|url|
		style(format!("\nPlease consult the source repository at {url} to assess production suitability and licensing restrictions.")).dim()
	);
	//println!(format!("NOTE: the resulting contract is not guaranteed to be audited or reviewed
	// for security vulnerabilities.{}",repository.unwrap()));
	cli.warning(format!("NOTE: the resulting contract is not guaranteed to be audited or reviewed for security vulnerabilities.{}",
					repository.unwrap_or_else(|| style("".to_string()))))?;

	// add next steps
	let mut next_steps = vec![
		format!("cd into {:?} and enjoy hacking! 🚀", contract_path.display()),
		"Use `pop build` to build your contract.".into(),
	];
	next_steps.push(format!("Use `pop up contract` to deploy your contract to a live network."));
	let next_steps: Vec<_> = next_steps
		.iter()
		.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
		.collect();
	cli.success(format!("Next Steps:\n{}", next_steps.join("\n")))?;

	cli.outro(format!(
		"Need help? Learn more at {}\n",
		style("https://learn.onpop.io").magenta().underlined()
	))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::fs;

	use super::*;
	use crate::{
		cli::MockCli,
		commands::new::{Command::Contract, NewArgs},
		new::contract::{guide_user_to_generate_contract, is_template_supported},
		Cli,
		Command::New,
	};
	use anyhow::Result;
	use clap::Parser;
	use console::style;
	use pop_common::templates::{Template, Type};
	use pop_contracts::{Contract as ContractTemplate, ContractType};
	use strum::VariantArray;
	use tempfile::tempdir;

	#[tokio::test]
	async fn new_contract_command_execute_with_defaults_works() -> Result<()> {
		let dir = tempdir()?;
		let dir_path = format!("{}/test_contract", dir.path().display().to_string());
		let cli = Cli::parse_from(["pop", "new", "contract", &dir_path]);

		let New(NewArgs { command: Contract(command) }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		command.execute().await?;
		Ok(())
	}

	#[tokio::test]
	async fn new_contract_template_command_works() -> Result<()> {
		let dir = tempdir()?;
		let dir_path = format!("{}/test_contract", dir.path().display().to_string());
		let cli =
			Cli::parse_from(["pop", "new", "contract", &dir_path, "-c", "erc", "-t", "erc20"]);

		let New(NewArgs { command: Contract(command) }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		command.execute().await?;
		Ok(())
	}

	#[tokio::test]
	async fn guide_user_to_generate_contract_works() -> anyhow::Result<()> {
		let mut items_select_contract_type: Vec<(String, String)> = Vec::new();
		for contract_type in ContractType::VARIANTS {
			items_select_contract_type.push((
				contract_type.name().to_string(),
				format!(
					"{} {} available option(s)",
					contract_type.description(),
					contract_type.templates().len(),
				),
			));
		}
		let mut items_select_contract: Vec<(String, String)> = Vec::new();
		for contract_template in ContractType::Erc.templates() {
			items_select_contract.push((
				contract_template.name().to_string(),
				contract_template.description().to_string(),
			));
		}
		let mut cli = MockCli::new()
			.expect_intro("Generate a contract")
			.expect_input("Where should your project be created?", "./erc20".into())
			.expect_select::<ContractTemplate>(
				"Select the contract:",
				Some(false),
				true,
				Some(items_select_contract),
				1, // "ERC20"
			)
			.expect_select::<ContractType>(
				"Select a template type:",
				Some(false),
				true,
				Some(items_select_contract_type),
				2, // "ERC"
			);

		let user_input = guide_user_to_generate_contract(&mut cli).await?;
		assert_eq!(user_input.name, Some("./erc20".to_string()));
		assert_eq!(user_input.contract_type, Some(ContractType::Erc));
		assert_eq!(user_input.template, Some(ContractTemplate::ERC20));

		cli.verify()?;
		Ok(())
	}

	#[test]
	fn generate_contract_from_template_works() -> anyhow::Result<()> {
		let dir = tempdir()?;
		let contract_path = dir.path().join("test_contract");
		let next_steps: Vec<_> = vec![
			format!("cd into {:?} and enjoy hacking! 🚀", contract_path.display()),
			"Use `pop build` to build your contract.".into(),
			"Use `pop up contract` to deploy your contract to a live network.".into(),
		]
		.iter()
		.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
		.collect();
		let mut cli = MockCli::new().expect_intro("Generating \"my_contract\" using Erc20!")
		.expect_success("Generation complete")
		.expect_warning(
			format!("NOTE: the resulting contract is not guaranteed to be audited or reviewed for security vulnerabilities.{}", 
			style(format!("\nPlease consult the source repository at {} to assess production suitability and licensing restrictions.", ContractTemplate::ERC20.repository_url().unwrap())).dim()))
		.expect_success(format!("Next Steps:\n{}", next_steps.join("\n")))
		.expect_outro(format!(
			"Need help? Learn more at {}\n",
			style("https://learn.onpop.io").magenta().underlined()
		));
		generate_contract_from_template(
			"my_contract",
			&contract_path,
			&ContractTemplate::ERC20,
			&mut cli,
		)?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn is_template_supported_works() -> Result<()> {
		is_template_supported(&ContractType::Erc, &ContractTemplate::ERC20)?;
		is_template_supported(&ContractType::Erc, &ContractTemplate::ERC721)?;
		assert!(
			is_template_supported(&ContractType::Erc, &ContractTemplate::CrossContract).is_err()
		);
		assert!(is_template_supported(&ContractType::Erc, &ContractTemplate::PSP22).is_err());
		is_template_supported(&ContractType::Examples, &ContractTemplate::Standard)?;
		is_template_supported(&ContractType::Examples, &ContractTemplate::CrossContract)?;
		assert!(is_template_supported(&ContractType::Examples, &ContractTemplate::ERC20).is_err());
		assert!(is_template_supported(&ContractType::Examples, &ContractTemplate::PSP22).is_err());
		is_template_supported(&ContractType::Psp, &ContractTemplate::PSP22)?;
		is_template_supported(&ContractType::Psp, &ContractTemplate::PSP34)?;
		assert!(is_template_supported(&ContractType::Psp, &ContractTemplate::ERC20).is_err());
		assert!(is_template_supported(&ContractType::Psp, &ContractTemplate::Standard).is_err());
		Ok(())
	}
}
