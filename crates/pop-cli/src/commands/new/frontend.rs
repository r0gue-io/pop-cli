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
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Supported package managers for frontend projects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
pub enum PackageManager {
	Pnpm,
	Bun,
	Yarn,
	Npm,
}

impl PackageManager {
	/// Get the package manager binary name. Used for checking if installed.
	pub fn name(&self) -> &'static str {
		match self {
			Self::Pnpm => "pnpm",
			Self::Bun => "bun",
			Self::Yarn => "yarn",
			Self::Npm => "npm",
		}
	}

	/// Get the executor command for running packages.
	pub fn command(&self) -> &'static str {
		match self {
			Self::Pnpm => "pnpm",
			Self::Bun => "bunx",
			Self::Yarn => "yarn",
			Self::Npm => "npx",
		}
	}

	/// Get the flag for executing packages (if any).
	pub fn flag(&self) -> Option<&'static str> {
		match self {
			Self::Pnpm => Some("dlx"),
			Self::Bun => None,
			Self::Yarn => Some("dlx"),
			Self::Npm => Some("-y"),
		}
	}

	/// Auto-detect which package manager is available. Detection priority: pnpm -> bun -> yarn ->
	/// npm
	pub fn detect() -> Result<Self> {
		if has("pnpm") {
			Ok(Self::Pnpm)
		} else if has("bun") {
			Ok(Self::Bun)
		} else if has("yarn") {
			Ok(Self::Yarn)
		} else if has("npm") {
			Ok(Self::Npm)
		} else {
			Err(anyhow::anyhow!(
				"No supported package manager found. Please install pnpm, bun, yarn, or npm."
			))
		}
	}
}

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
/// * `package_manager` - Optional package manager to use. If None, auto-detects.
/// * `cli`: Command line interface.
pub async fn create_frontend(
	target: &Path,
	template: &FrontendTemplate,
	package_manager: Option<PackageManager>,
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
			if matches!(package_manager, Some(pm) if pm != PackageManager::Bun) {
				cli.warning("Inkathon template requires bun. Ignoring specified package manager.")?;
			}
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
				package_manager,
			)?;

			cmd(&args[0], &args[1..]).dir(&project_dir).run()?;
		},
		FrontendTemplate::CreateDotApp => {
			let args =
				build_command(command, &["frontend", "--template", "react-papi"], package_manager)?;

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
/// * `package_manager` - Optional package manager to use. If None, auto-detects.
fn build_command(
	package: &str,
	args: &[&str],
	package_manager: Option<PackageManager>,
) -> Result<Vec<String>> {
	let manager = if let Some(pm) = package_manager {
		if !has(pm.name()) {
			return Err(anyhow::anyhow!(
				"Specified package manager '{}' not found. Please install it first.",
				pm.name()
			));
		}
		pm
	} else {
		PackageManager::detect()?
	};

	// Build command
	let mut result = vec![manager.command().to_string()];
	if let Some(flag) = manager.flag() {
		result.push(flag.to_string());
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
	fn build_command_works() -> anyhow::Result<()> {
		let package = "create-typink";
		let args = &["--name", "frontend"];

		let result = build_command(package, args, Some(PackageManager::Npm))?;

		assert_eq!(result[0], "npx");
		assert_eq!(result[1], "-y");
		assert_eq!(result[2], package);
		assert_eq!(result.last().unwrap(), "frontend");

		Ok(())
	}
}
