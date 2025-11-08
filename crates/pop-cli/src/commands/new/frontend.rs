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
/// If only one template is available, it is automatically selected without prompting.
///
/// # Arguments
/// * `frontend_type`: The filter that determines which frontend templates are shown.
/// * `cli`: Command line interface.
pub fn prompt_frontend_template(
	frontend_type: &FrontendType,
	cli: &mut impl cli::traits::Cli,
) -> Result<FrontendTemplate> {
	let templates = frontend_type.templates();

	// If there's only one template, return it without prompting
	if templates.len() == 1 {
		return Ok(templates[0].clone());
	}
	let mut prompt =
		cli.select(format!("Select a frontend template for your {}:", frontend_type.name()));
	for (i, template) in templates.into_iter().enumerate() {
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
pub async fn create_frontend(
	target: &Path,
	template: &FrontendTemplate,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	ensure_node_v20(false, cli).await?;
	ensure_npx()?;
	let project_dir = target.canonicalize()?;
	let command = template
		.command()
		.ok_or_else(|| anyhow::anyhow!("no command configured for {:?}", template))?;
	match template {
		// Inkathon requires Bun installed.
		FrontendTemplate::Inkathon => {
			let bun = ensure_bun(false, cli).await?;
			cmd(&bun, &["add", "polkadot-api"]).dir(&project_dir).unchecked().run()?;
			cmd(&bun, &["x", command, "frontend", "--yes"])
				.dir(&project_dir)
				.env("SKIP_INSTALL_SIMPLE_GIT_HOOKS", "1")
				.unchecked()
				.run()?;
		},
		FrontendTemplate::Typink => {
			cmd(
				"npx",
				vec![
					"-y",
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
		FrontendTemplate::CreateDotApp => {
			cmd("npx", vec!["-y", command, "frontend", "--template", "react-papi"])
				.dir(&project_dir)
				.run()?;
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
		// Chain only has one template, so it should be selected automatically without prompting
		let mut cli = MockCli::new();

		let user_input = prompt_frontend_template(&FrontendType::Chain, &mut cli)?;
		assert_eq!(user_input, FrontendTemplate::CreateDotApp);

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
