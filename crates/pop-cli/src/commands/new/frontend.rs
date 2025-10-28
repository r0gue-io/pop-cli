// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::*},
	install::frontend::{ensure_bun, ensure_node_v20, ensure_npx},
};
use anyhow::Result;
use duct::cmd;
use pop_common::{
	FrontendTemplate, FrontendType,
	templates::{Template, Type},
};
use std::path::Path;

/// Prompts the user to pick a frontend template for the given frontend type (Chain or Contract).
///
/// # Arguments
/// * `frontend_type`: The filter that determines which frontend templates are shown.
/// * `cli`: Command line interface.
pub fn prompt_frontend_template(
	frontend_type: &FrontendType,
	cli: &mut impl cli::traits::Cli,
) -> Result<FrontendTemplate> {
	let mut prompt =
		cli.select(format!("Select a frontend template for your {}:", frontend_type.name()));
	for (i, template) in frontend_type.templates().into_iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(template);
		}
		prompt = prompt.item(template, template.name(), template.description().trim());
	}
	Ok(prompt.interact()?.clone())
}

/// Create a new frontend.
///
/// # Arguments
/// * `target` - Location where the smart contract will be created.
/// * `template` - Frontend template to generate the contract from.
/// * `cli`: Command line interface.
pub fn create_frontend(
	target: &Path,
	template: &FrontendTemplate,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	ensure_node_v20()?;
	ensure_npx()?;
	let project_dir = target.canonicalize()?;
	let command = template
		.command()
		.ok_or_else(|| anyhow::anyhow!("no command configured for {:?}", template))?;
	match template {
		// Inkathon requires Bun installed.
		FrontendTemplate::Inkathon => {
			let bun = ensure_bun(cli)?;
			cmd(&bun, &["add", "polkadot-api"]).dir(&project_dir).unchecked().run()?;
			cmd(&bun, &[command, "frontend", "--yes"])
				.dir(&project_dir)
				.env("SKIP_INSTALL_SIMPLE_GIT_HOOKS", "1")
				.unchecked()
				.run()?;
		},
		// Typeink we can specify the parameters directly
		FrontendTemplate::Typink => {
			cmd(
				"npx",
				vec![
					command,
					"--name",
					"frontend",
					"--template",
					"inkv6-nextjs",
					"--networks",
					"Passet Hub",
					"--no-git",
				],
			)
			.dir(&project_dir)
			.run()?;
		},
		_ => {
			cmd("npx", vec![command, "frontend"]).dir(&project_dir).run()?;
		},
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use cli::MockCli;

	#[test]
	fn prompt_chain_frontend_template_works() -> anyhow::Result<()> {
		let mut items_select_template: Vec<(String, String)> = Vec::new();
		for template in FrontendType::Chain.templates() {
			items_select_template
				.push((template.name().to_string(), template.description().to_string()));
		}
		let mut cli = MockCli::new().expect_select(
			"Select a frontend template for your Chain:",
			Some(false),
			true,
			Some(items_select_template),
			0,
			None,
		);

		let user_input = prompt_frontend_template(&FrontendType::Chain, &mut cli)?;
		assert_eq!(user_input, FrontendTemplate::CreatePolkadotDapp);

		cli.verify()
	}

	#[test]
	fn prompt_contract_frontend_template_works() -> anyhow::Result<()> {
		let mut items_select_template: Vec<(String, String)> = Vec::new();
		for template in FrontendType::Contract.templates() {
			items_select_template
				.push((template.name().to_string(), template.description().to_string()));
		}
		let mut cli = MockCli::new().expect_select(
			"Select a frontend template for your Contract:",
			Some(false),
			true,
			Some(items_select_template),
			0,
			None,
		);

		let user_input = prompt_frontend_template(&FrontendType::Contract, &mut cli)?;
		assert_eq!(user_input, FrontendTemplate::Typink);

		cli.verify()
	}
}
