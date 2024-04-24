// SPDX-License-Identifier: GPL-3.0
use crate::{
	helpers::{
		check_destination_path, display_release_versions_to_user, display_select_options,
		get_customization_value, is_template_supported, prompt_customizable_options,
	},
	style::{style, Theme},
};
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

use cliclack::{clear_screen, input, intro, log, outro, outro_cancel, set_theme};
use pop_parachains::{instantiate_template_dir, Config, Git, GitHub, Provider, Template};

#[derive(Args)]
pub struct NewParachainCommand {
	#[arg(help = "Name of the project. If empty assistance in the process will be provided.")]
	pub(crate) name: Option<String>,
	#[arg(help = "Template provider. Options are pop or parity", default_value = "pop")]
	pub(crate) provider: Option<Provider>,
	#[arg(
		short = 't',
		long,
		help = "Template to use: 'base' for Pop and 'cpt' and 'fpt' for Parity templates"
	)]
	pub(crate) template: Option<Template>,
	#[arg(long, short, help = "Token Symbol", default_value = "UNIT")]
	pub(crate) symbol: Option<String>,
	#[arg(long, short, help = "Token Decimals", default_value = "12")]
	pub(crate) decimals: Option<u8>,
	#[arg(
		long = "endowment",
		short,
		help = "Token Endowment for dev accounts",
		default_value = "1u64 << 60"
	)]
	pub(crate) initial_endowment: Option<String>,
	#[arg(
		short = 'p',
		long,
		help = "Path for the parachain project, [default: current directory]"
	)]
	pub(crate) path: Option<PathBuf>,
}

impl NewParachainCommand {
	pub(crate) async fn execute(&self) -> Result<()> {
		clear_screen()?;
		set_theme(Theme);

		return match &self.name {
			// If user doesn't select the name guide them to generate a parachain.
			None => guide_user_to_generate_parachain().await,
			Some(name) => {
				let provider = &self.provider.clone().unwrap_or_default();
				let template = match &self.template {
					Some(template) => template.clone(),
					None => provider.default_template(), // Each provider has a template by default
				};

				is_template_supported(provider, &template)?;
				let config = get_customization_value(
					&template,
					self.symbol.clone(),
					self.decimals,
					self.initial_endowment.clone(),
				)?;

				generate_parachain_from_template(name, provider, &template, None, config)
			},
		};
	}
}

fn generate_parachain_from_template(
	name_template: &String,
	provider: &Provider,
	template: &Template,
	tag_version: Option<String>,
	config: Config,
) -> Result<()> {
	intro(format!(
		"{}: Generating \"{}\" using {:?} from {:?}!",
		style(" Pop CLI ").black().on_magenta(),
		name_template,
		template,
		provider
	))?;
	let destination_path = check_destination_path(name_template)?;

	let mut spinner = cliclack::spinner();
	spinner.start("Generating parachain...");
	let tag = instantiate_template_dir(template, destination_path, tag_version, config)?;
	if let Err(err) = Git::git_init(destination_path, "initialized parachain") {
		if err.class() == git2::ErrorClass::Config && err.code() == git2::ErrorCode::NotFound {
			outro_cancel("git signature could not be found. Please configure your git config with your name and email")?;
		}
	}
	spinner.stop("Generation complete");
	if let Some(tag) = tag {
		log::info(format!("Version: {}", tag))?;
	}

	cliclack::note(
			"NOTE: the resulting parachain is not guaranteed to be audited or reviewed for security vulnerabilities.",
		format!("Please consult the source repository at {} to assess production suitability and licensing restrictions.", template.repository_url()?))?;

	outro(format!("cd into \"{}\" and enjoy hacking! 🚀", name_template))?;

	Ok(())
}

async fn guide_user_to_generate_parachain() -> Result<()> {
	intro(format!("{}: Generate a parachain", style(" Pop CLI ").black().on_magenta()))?;

	let mut prompt = cliclack::select("Select a template provider: ".to_string());
	for (i, provider) in Provider::providers().iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(provider);
		}
		prompt = prompt.item(
			provider,
			provider.name(),
			format!(
				"{} {} available option(s)",
				provider.description(),
				provider.templates().len()
			),
		);
	}
	let provider = prompt.interact()?;
	let template = display_select_options(provider)?;

	let url = url::Url::parse(&["https://github.com/", template.repository_url()?].concat())
		.expect("valid repository url");
	let latest_3_releases = GitHub::get_latest_n_releases(3, &url).await?;

	let mut release_name = None;
	if latest_3_releases.len() > 0 {
		release_name = Some(display_release_versions_to_user(latest_3_releases)?);
	}

	let name: String = input("Where should your project be created?")
		.placeholder("./my-parachain")
		.default_input("./my-parachain")
		.interact()?;

	let mut customizable_options = Config {
		symbol: "UNIT".to_string(),
		decimals: 12,
		initial_endowment: "1u64 << 60".to_string(),
	};
	if matches!(template, Template::Base) {
		customizable_options = prompt_customizable_options()?;
	}

	clear_screen()?;

	generate_parachain_from_template(
		&name,
		&provider,
		&template,
		release_name,
		customizable_options,
	)
}

#[cfg(test)]
mod tests {

	use git2::Repository;

	use super::*;
	use std::{fs, path::Path};

	#[tokio::test]
	async fn test_new_parachain_command_execute() -> anyhow::Result<()> {
		let command = NewParachainCommand {
			name: Some("test_parachain".to_string()),
			provider: Some(Provider::Pop),
			template: Some(Template::Base),
			symbol: Some("UNIT".to_string()),
			decimals: Some(12),
			initial_endowment: Some("1u64 << 60".to_string()),
			path: None,
		};
		let result = command.execute().await;
		assert!(result.is_ok());

		// check for git_init
		let repo = Repository::open(Path::new(&command.name.unwrap()))?;
		let reflog = repo.reflog("HEAD")?;
		assert_eq!(reflog.len(), 1);

		// Clean up
		if let Err(err) = fs::remove_dir_all("test_parachain") {
			eprintln!("Failed to delete directory: {}", err);
		}
		Ok(())
	}
}
