// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self, Cli,
		traits::{Cli as _, *},
	},
	common::helpers::check_destination_path,
	new::frontend::{PackageManager, create_frontend, prompt_frontend_template},
};

use anyhow::Result;
use clap::{
	Args,
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
};
use console::style;
use pop_common::{
	FrontendTemplate, FrontendType, enum_variants, get_project_name_from_path, templates::Template,
};
use pop_contracts::{Contract, create_smart_contract, is_valid_contract_name};
use serde::Serialize;
use std::{
	fs,
	path::{Path, PathBuf},
	str::FromStr,
};
use strum::VariantArray;

#[derive(Args, Clone, Serialize)]
#[cfg_attr(test, derive(Default))]
pub struct NewContractCommand {
	/// The name of the contract.
	pub(crate) name: Option<String>,
	/// The template to use.
	#[arg(short, long, value_parser = enum_variants!(Contract))]
	pub(crate) template: Option<Contract>,
	/// List available templates.
	#[arg(short, long)]
	pub(crate) list: bool,
	/// Also scaffold a frontend. Optionally specify template, if flag provided without value,
	/// prompts for template selection.
	#[arg(
		short = 'f',
		long = "with-frontend",
		value_name = "TEMPLATE_NAME",
		num_args = 0..=1,
		require_equals = true,
		value_parser = ["typink", "inkathon"]
	)]
	pub(crate) with_frontend: Option<String>,
	/// Package manager to use for frontend. If not specified, auto-detects based on what's
	/// installed.
	#[arg(long = "package-manager", value_name = "MANAGER", requires = "with_frontend")]
	pub(crate) package_manager: Option<PackageManager>,
}

impl NewContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(&mut self) -> Result<()> {
		let mut cli = Cli;

		if self.list {
			cli.intro("Available templates")?;
			for template in Contract::templates() {
				if !template.is_deprecated() {
					cli.info(format!("{}: {}", template.name(), template.description()))?;
				}
			}
			return Ok(());
		}

		// Prompt for missing fields interactively
		if self.name.is_none() || self.template.is_none() {
			guide_user_to_generate_contract(&mut cli, self).await?;
		}

		let path_project = self.name.as_ref().expect("name can not be none; qed");
		let path = Path::new(path_project);
		let name = get_project_name_from_path(path, "my-contract");

		// Validate contract name.
		if let Err(e) = is_valid_contract_name(&name) {
			cli.outro_cancel(e)?;
			return Ok(());
		}

		let template = self.template.clone().unwrap_or_default();
		let mut frontend_template: Option<FrontendTemplate> = None;
		if let Some(frontend_arg) = &self.with_frontend {
			frontend_template =
				if frontend_arg.is_empty() {
					// User provided --with-frontend without value: prompt for template
					Some(prompt_frontend_template(&FrontendType::Contract, &mut cli)?)
				} else {
					// User specified a template explicitly: parse and use it
					Some(FrontendTemplate::from_str(frontend_arg).map_err(|_| {
						anyhow::anyhow!("Invalid frontend template: {}", frontend_arg)
					})?)
				};
		}
		let contract_path = generate_contract_from_template(
			&name,
			path,
			&template,
			frontend_template,
			self.package_manager,
			&mut cli,
		)
		.await?;

		// If the contract is part of a workspace, add it to that workspace
		if let Some(workspace_toml) = rustilities::manifest::find_workspace_manifest(path) {
			// Canonicalize paths before passing to rustilities to avoid strip_prefix errors
			// This ensures paths are absolute and consistent, especially when using simple names
			rustilities::manifest::add_crate_to_workspace(
				&workspace_toml.canonicalize()?,
				&contract_path.canonicalize()?,
			)?;
		}

		Ok(())
	}
}

/// Guide the user to provide any missing fields for contract generation.
async fn guide_user_to_generate_contract(
	cli: &mut impl cli::traits::Cli,
	command: &mut NewContractCommand,
) -> Result<()> {
	cli.intro("Generate a contract")?;

	if command.template.is_none() {
		let template = display_select_options(cli)?;
		command.template = Some(template);
	}

	if command.name.is_none() {
		let name: String = cli
			.input("Where should your project be created?")
			.placeholder("./my-contract")
			.default_input("./my-contract")
			.interact()?;
		command.name = Some(name);
	}

	if command.with_frontend.is_none() {
		command.with_frontend = if cli
			.confirm("Would you like to scaffold a frontend template as well?".to_string())
			.initial_value(true)
			.interact()?
		{
			Some(String::new()) // Empty string means prompt for template
		} else {
			None
		};
	}

	Ok(())
}

fn display_select_options(cli: &mut impl cli::traits::Cli) -> Result<Contract> {
	let mut prompt = cli.select("Select a template:".to_string());
	for (i, template) in Contract::templates().iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(template);
		}
		prompt = prompt.item(template, template.name(), template.description());
	}
	Ok(prompt.interact()?.clone())
}

async fn generate_contract_from_template(
	name: &str,
	path: &Path,
	template: &Contract,
	frontend_template: Option<FrontendTemplate>,
	package_manager: Option<PackageManager>,
	cli: &mut impl cli::traits::Cli,
) -> anyhow::Result<PathBuf> {
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
	next_steps.push("Use `pop up` to deploy your contract to a live network.".to_string());

	if let Some(frontend_template) = &frontend_template {
		create_frontend(contract_path.as_path(), frontend_template, package_manager, cli).await?;
		next_steps.push(format!(
			"Frontend template created inside {}. To run it locally, use: `pop up frontend`. Navigate to the `frontend` folder to start customizing it for your contract. ", contract_path.display()
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
	Ok(contract_path)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		Cli,
		Command::New,
		cli::MockCli,
		commands::new::{Command::Contract, NewArgs},
	};
	use anyhow::Result;
	use clap::Parser;
	use console::style;
	use pop_common::templates::Template;
	use pop_contracts::Contract as ContractTemplate;
	use strum::VariantArray;
	use tempfile::tempdir;

	#[tokio::test]
	async fn test_new_contract_command_execute_with_defaults_executes() -> Result<()> {
		let dir = tempdir()?;
		let dir_path = format!("{}/test_contract", dir.path().display());
		let cli = Cli::parse_from(["pop", "new", "contract", &dir_path, "--template", "standard"]);

		let New(NewArgs { command: Some(Contract(mut command)), .. }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		command.execute().await?;
		Ok(())
	}

	#[tokio::test]
	async fn guide_user_to_generate_contract_works() -> anyhow::Result<()> {
		let mut items_select_contract: Vec<(String, String)> = Vec::new();
		for contract_template in ContractTemplate::VARIANTS {
			items_select_contract.push((
				contract_template.name().to_string(),
				contract_template.description().to_string(),
			));
		}
		let mut cli = MockCli::new()
			.expect_intro("Generate a contract")
			.expect_select(
				"Select a template:",
				Some(false),
				true,
				Some(items_select_contract),
				1, // "erc20"
				None,
			)
			.expect_confirm("Would you like to scaffold a frontend template as well?", true)
			.expect_input("Where should your project be created?", "./erc20".into());

		let mut user_input: NewContractCommand = Default::default();
		guide_user_to_generate_contract(&mut cli, &mut user_input).await?;
		assert_eq!(user_input.name, Some("./erc20".to_string()));
		assert_eq!(user_input.template, Some(ContractTemplate::ERC20));
		assert_eq!(user_input.with_frontend, Some(String::new())); // Empty string means prompt

		cli.verify()
	}

	#[tokio::test]
	async fn generate_contract_from_template_works() -> anyhow::Result<()> {
		let dir = tempdir()?;
		let contract_path = dir.path().join("test_contract");
		let next_steps: Vec<_> = [
			format!("cd into {:?} and enjoy hacking! 🚀", contract_path.display()),
			"Use `pop build` to build your contract.".into(),
			"Use `pop up` to deploy your contract to a live network.".into(),
		]
		.iter()
		.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
		.collect();
		let mut cli = MockCli::new().expect_intro("Generating \"my-contract\" using Erc20!")
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
			"my-contract",
			&contract_path,
			&ContractTemplate::ERC20,
			None,
			None,
			&mut cli,
		)
		.await?;
		cli.verify()
	}

	#[tokio::test]
	async fn test_new_contract_list_templates() -> Result<()> {
		let mut command = NewContractCommand { list: true, ..Default::default() };
		// Just ensure it can be executed without error.
		// Since it uses the real Cli internally, we can't easily verify output in this unit test
		// without refactoring execute to take a Cli trait object.
		command.execute().await?;
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
			"pop",
			"new",
			"contract",
			"flipper",
			"--template",
			"standard", // Just the name, not a path like "./flipper"
		]);
		let New(NewArgs { command: Some(Contract(mut command)), .. }) = cli.command else {
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
