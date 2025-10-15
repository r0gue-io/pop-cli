// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self, Cli,
		traits::{Cli as _, *},
	},
	common::helpers::check_destination_path,
	new::frontend::{create_frontend, prompt_frontend_template},
};

use anyhow::Result;
use clap::{
	Args,
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
};
use console::style;
use pop_common::{
	FrontendTemplate, FrontendType, enum_variants, get_project_name_from_path,
	templates::{Template, Type},
};
use pop_contracts::{Contract, ContractType, create_smart_contract, is_valid_contract_name};
use std::{fs, path::Path, str::FromStr};
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
	#[arg(
		short = 'f',
		long = "with-frontend",
		help = "Also scaffold a frontend. If a value is provided, it will be preselected in the prompt."
	)]
	pub(crate) with_frontend: bool,
}

impl NewContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<Contract> {
		let mut cli = Cli;
		// If the user doesn't provide a name, guide them in generating a contract.
		let contract_config = if self.name.is_none() {
			guide_user_to_generate_contract(&mut cli).await?
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
			cli.outro_cancel(e)?;
			return Ok(Contract::Standard);
		}

		let contract_type = &contract_config.contract_type.clone().unwrap_or_default();
		let template = match &contract_config.template {
			Some(template) => template.clone(),
			None => contract_type.default_template().expect("contract types have defaults; qed."), /* Default contract type */
		};

		is_template_supported(contract_type, &template)?;
		let mut frontend_template: Option<FrontendTemplate> = None;
		if self.with_frontend {
			frontend_template =
				Some(prompt_frontend_template(&FrontendType::Contract, &mut cli::Cli)?);
		}
		generate_contract_from_template(name, path, &template, frontend_template, &mut cli)?;

		// If the contract is part of a workspace, add it to that workspace
		if let Some(workspace_toml) = rustilities::manifest::find_workspace_manifest(path) {
			// Canonicalize paths before passing to rustilities to avoid strip_prefix errors
			// This ensures paths are absolute and consistent, especially when using simple names
			rustilities::manifest::add_crate_to_workspace(
				&workspace_toml.canonicalize()?,
				&path.canonicalize()?,
			)?;
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
async fn guide_user_to_generate_contract(
	cli: &mut impl cli::traits::Cli,
) -> Result<NewContractCommand> {
	cli.intro("Generate a contract")?;

	let contract_type = {
		let mut contract_type_prompt = cli.select("Select a template type: ".to_string());
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
	let template = display_select_options(contract_type, cli)?;

	// Prompt for location.
	let name: String = cli
		.input("Where should your project be created?")
		.placeholder("./my_contract")
		.default_input("./my_contract")
		.interact()?;

	Ok(NewContractCommand {
		name: Some(name),
		contract_type: Some(contract_type.clone()),
		template: Some(template),
		// TODO: Prompt if not indicated?
		with_frontend: false,
	})
}

fn display_select_options(
	contract_type: &ContractType,
	cli: &mut impl cli::traits::Cli,
) -> Result<Contract> {
	let mut prompt = cli.select("Select the contract:".to_string());
	for (i, template) in contract_type.templates().into_iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(template);
		}
		prompt = prompt.item(template, template.name(), template.description());
	}
	Ok(prompt.interact()?.clone())
}

fn generate_contract_from_template(
	name: &str,
	path: &Path,
	template: &Contract,
	frontend_template: Option<FrontendTemplate>,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<()> {
	cli.intro(format!("Generating \"{}\" using {}!", name, template.name(),))?;

	let contract_path = check_destination_path(path, cli)?;
	fs::create_dir_all(contract_path.as_path())?;
	let spinner = cliclack::spinner();
	spinner.start("Generating contract...");
	create_smart_contract(name, contract_path.as_path(), template)?;
	spinner.clear();
	// Replace spinner with success.
	console::Term::stderr().clear_last_lines(2)?;
	cli.success("Generation complete")?;

	// warn about audit status and licensing
	let repository = template.repository_url().ok().map(|url|
		style(format!("\nPlease consult the source repository at {url} to assess production suitability and licensing restrictions.")).dim()
	);
	cli.warning(format!("NOTE: the resulting contract is not guaranteed to be audited or reviewed for security vulnerabilities.{}",
					repository.unwrap_or_else(|| style("".to_string()))))?;

	// add next steps
	let mut next_steps = vec![
		format!("cd into {:?} and enjoy hacking! 🚀", contract_path.display()),
		"Use `pop build` to build your contract.".into(),
	];
	next_steps.push("Use `pop up contract` to deploy your contract to a live network.".to_string());

	if let Some(frontend_template) = &frontend_template {
		create_frontend(contract_path.as_path(), frontend_template)?;
		next_steps.push(format!(
			"Frontend template {frontend_template} created inside {:?}. Go to the folder and follow the README instructions to get started.", contract_path.display()
		))
	};

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
	use super::*;
	use crate::{
		Cli,
		Command::New,
		cli::MockCli,
		commands::new::{Command::Contract, NewArgs},
		new::contract::{guide_user_to_generate_contract, is_template_supported},
	};
	use anyhow::Result;
	use clap::Parser;
	use console::style;
	use pop_common::templates::{Template, Type};
	use pop_contracts::{Contract as ContractTemplate, ContractType};
	use strum::VariantArray;
	use tempfile::tempdir;

	#[tokio::test]
	async fn test_new_contract_command_execute_with_defaults_executes() -> Result<()> {
		let dir = tempdir()?;
		let dir_path = format!("{}/test_contract", dir.path().display());
		let cli = Cli::parse_from(["pop", "new", "contract", &dir_path]);

		let New(NewArgs { command: Some(Contract(command)) }) = cli.command else {
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
			.expect_select(
				"Select a template type: ",
				Some(false),
				true,
				Some(items_select_contract_type),
				1, // "ERC",
				None,
			)
			.expect_select(
				"Select the contract:",
				Some(false),
				true,
				Some(items_select_contract),
				0, // "ERC20"
				None,
			);

		let user_input = guide_user_to_generate_contract(&mut cli).await?;
		assert_eq!(user_input.name, Some("./erc20".to_string()));
		assert_eq!(user_input.contract_type, Some(ContractType::Erc));
		assert_eq!(user_input.template, Some(ContractTemplate::ERC20));

		cli.verify()
	}

	#[test]
	fn generate_contract_from_template_works() -> anyhow::Result<()> {
		let dir = tempdir()?;
		let contract_path = dir.path().join("test_contract");
		let next_steps: Vec<_> = [
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
			None,
			&mut cli,
		)?;
		cli.verify()
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

	#[tokio::test]
	async fn test_contract_in_workspace_with_simple_name() -> Result<()> {
		// The bug occurs when you pass a simple name like "flipper" instead of "./flipper"
		// and the contract is created inside a workspace.
		let temp_dir = tempdir()?;
		let workspace_path = temp_dir.path();
		// Create a workspace Cargo.toml
		fs::write(
			workspace_path.join("Cargo.toml"),
			r#"[workspace]
resolver = "2"
members = []

[workspace.package]
edition = "2024"
"#,
		)?;
		// Change to the workspace directory to simulate real usage
		let original_dir = std::env::current_dir()?;
		std::env::set_current_dir(workspace_path)?;
		// User runs: pop new contract flipper -t standard
		// They pass just "flipper", not "./flipper"
		let cli = Cli::parse_from([
			"pop", "new", "contract", "flipper", // Just the name, not a path like "./flipper"
		]);
		let New(NewArgs { command: Some(Contract(command)) }) = cli.command else {
			panic!("unable to parse command")
		};
		let result = command.execute().await;
		// Restore original directory
		std::env::set_current_dir(original_dir)?;
		result?;
		// Verify the contract was created
		assert!(workspace_path.join("flipper").exists());
		assert!(workspace_path.join("flipper/Cargo.toml").exists());
		assert!(workspace_path.join("flipper/lib.rs").exists());
		// Verify it was added to the workspace
		let workspace_toml = fs::read_to_string(workspace_path.join("Cargo.toml"))?;
		assert!(
			workspace_toml.contains("flipper"),
			"Contract should be added to workspace members"
		);

		Ok(())
	}
}
