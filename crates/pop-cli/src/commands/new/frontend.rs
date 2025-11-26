// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::*},
	install::frontend::{ensure_bun, ensure_node_v20, has},
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
/// * `target` - Location where the frontend will be created.
/// * `template` - Frontend template to generate the contract from.
/// * `cli`: Command line interface.
pub async fn create_frontend(
	target: &Path,
	template: &FrontendTemplate,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	ensure_node_v20(false, cli).await?;
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
			let args = build_command(
				command,
				&[
					"--name",
					"frontend",
					"--template",
					"inkv6-nextjs",
					"--networks",
					"Passet Hub",
					"--no-git",
				],
			)?;

			cmd(&args[0], &args[1..]).dir(&project_dir).run()?;
		},
		FrontendTemplate::CreateDotApp => {
			let args = build_command(command, &["frontend", "--template", "react-papi"])?;

			cmd(&args[0], &args[1..]).dir(&project_dir).run()?;
		},
	}
	Ok(())
}

/// Build the complete command to create the frontend.
///
/// # Arguments
/// * `package` - The package to execute.
/// * `args` - Additional arguments to pass to the package
fn build_command(package: &str, args: &[&str]) -> Result<Vec<String>> {
	let mut result = Vec::new();

	if has("pnpm") {
		result.push("pnpm".to_string());
		result.push("dlx".to_string());
	} else if has("bun") {
		result.push("bunx".to_string());
	} else if has("yarn") {
		result.push("yarn".to_string());
		result.push("dlx".to_string());
	} else if has("npx") {
		result.push("npx".to_string());
		result.push("-y".to_string());
	} else {
		return Err(anyhow::anyhow!(
			"No supported package manager found. Please install pnpm, bun, yarn, or npm."
		));
	}

	result.push(package.to_string());
	result.extend(args.iter().map(|s| s.to_string()));

	Ok(result)
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

	#[test]
	fn build_command_works() {
		let result = build_command("create-typink", &["--name", "frontend"]);
		assert!(result.is_ok());

		let args = result.unwrap();
		assert!(args.len() >= 3);

		match args[0].as_str() {
			"pnpm" => {
				assert_eq!(args[1], "dlx");
				assert_eq!(args[2], "create-typink");
			},
			"bunx" => {
				assert_eq!(args[1], "create-typink");
			},
			"yarn" => {
				assert_eq!(args[1], "dlx");
				assert_eq!(args[2], "create-typink");
			},
			"npx" => {
				assert_eq!(args[1], "-y");
				assert_eq!(args[2], "create-typink");
			},
			_ => panic!("Unexpected runner: {}", args[0]),
		}
	}
}
