// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{
		self, Cli,
		traits::{Cli as _, *},
	},
	common::helpers::check_destination_path,
};

use anyhow::Result;
use clap::{
	Args,
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
};
use console::style;
use pop_common::{enum_variants, get_project_name_from_path, templates::Template};
use pop_contracts::{Contract, create_smart_contract, is_valid_contract_name};
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
	/// The template to use.
	#[arg(short, long, value_parser = enum_variants!(Contract))]
	pub(crate) template: Option<Contract>,
}

impl NewContractCommand {
	/// Executes the command.
	pub(crate) async fn execute(mut self) -> Result<Contract> {
		let mut cli = Cli;

		// Prompt for missing fields interactively
		if self.name.is_none() || self.template.is_none() {
			cli.intro("Generate a contract")?;

			// Prompt for template if not provided
			if self.template.is_none() {
				let template = {
					let mut template_prompt = cli.select("Select a template:".to_string());
					for (i, template) in Contract::templates().iter().enumerate() {
						if i == 0 {
							template_prompt = template_prompt.initial_value(template);
						}
						template_prompt =
							template_prompt.item(template, template.name(), template.description());
					}
					template_prompt.interact()?.clone()
				};
				self.template = Some(template);
			}

			// Prompt for name if not provided
			if self.name.is_none() {
				let name: String = cli
					.input("Where should your project be created?")
					.placeholder("./my_contract")
					.default_input("./my_contract")
					.interact()?;
				self.name = Some(name);
			}
		}

		let path_project = self.name.as_ref().expect("name can not be none; qed");
		let path = Path::new(path_project);
		let name = get_project_name_from_path(path, "my_contract");

		// Validate contract name.
		if let Err(e) = is_valid_contract_name(name) {
			cli.outro_cancel(e)?;
			return Ok(Contract::Standard);
		}

		let template = self.template.unwrap_or_default();

		let contract_path = generate_contract_from_template(name, path, &template, &mut cli)?;

		// If the contract is part of a workspace, add it to that workspace
		if let Some(workspace_toml) = rustilities::manifest::find_workspace_manifest(path) {
			// Canonicalize paths before passing to rustilities to avoid strip_prefix errors
			// This ensures paths are absolute and consistent, especially when using simple names
			rustilities::manifest::add_crate_to_workspace(
				&workspace_toml.canonicalize()?,
				&contract_path.canonicalize()?,
			)?;
		}

		Ok(template)
	}
}

fn generate_contract_from_template(
	name: &str,
	path: &Path,
	template: &Contract,
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
	next_steps.push("Use `pop up contract` to deploy your contract to a live network.".to_string());
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
	use tempfile::tempdir;

	#[tokio::test]
	async fn test_new_contract_command_execute_with_defaults_executes() -> Result<()> {
		let dir = tempdir()?;
		let dir_path = format!("{}/test_contract", dir.path().display());
		let cli = Cli::parse_from(["pop", "new", "contract", &dir_path, "--template", "standard"]);

		let New(NewArgs { command: Some(Contract(command)) }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		command.execute().await?;
		Ok(())
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
			&mut cli,
		)?;
		cli.verify()
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
