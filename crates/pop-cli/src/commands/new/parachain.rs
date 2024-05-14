// SPDX-License-Identifier: GPL-3.0
use crate::style::{style, Theme};
use anyhow::Result;
use clap::{
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
	Args,
};
use std::{fs, path::Path, str::FromStr};

use cliclack::{clear_screen, confirm, input, intro, log, outro, outro_cancel, set_theme};
use pop_parachains::{
	instantiate_template_dir, is_initial_endowment_valid, Config, Git, GitHub, Provider, Release,
	Template,
};
use strum::VariantArray;

const DEFAULT_INITIAL_ENDOWMENT: &str = "1u64 << 60";

#[derive(Args, Clone)]
pub struct NewParachainCommand {
	#[arg(help = "Name of the project. If empty assistance in the process will be provided.")]
	pub(crate) name: Option<String>,
	#[arg(
		help = "Template provider.",
		default_value = Provider::Pop.as_ref(),
		value_parser = crate::enum_variants!(Provider)
	)]
	pub(crate) provider: Option<Provider>,
	#[arg(
		short = 't',
		long,
		help = "Template to use.",
		value_parser = crate::enum_variants!(Template)
	)]
	pub(crate) template: Option<Template>,
	#[arg(
		short = 'r',
		long,
		help = "Release tag to use for template. If empty, latest release will be used."
	)]
	pub(crate) release_tag: Option<String>,
	#[arg(long, short, help = "Token Symbol", default_value = "UNIT")]
	pub(crate) symbol: Option<String>,
	#[arg(long, short, help = "Token Decimals", default_value = "12")]
	pub(crate) decimals: Option<u8>,
	#[arg(
		long = "endowment",
		short,
		help = "Token Endowment for dev accounts",
		default_value = DEFAULT_INITIAL_ENDOWMENT
	)]
	pub(crate) initial_endowment: Option<String>,
}

#[macro_export]
macro_rules! enum_variants {
	($e: ty) => {{
		PossibleValuesParser::new(
			<$e>::VARIANTS
				.iter()
				.map(|p| PossibleValue::new(p.as_ref()))
				.collect::<Vec<_>>(),
		)
		.try_map(|s| {
			<$e>::from_str(&s).map_err(|e| format!("could not convert from {s} to provider"))
		})
	}};
}

impl NewParachainCommand {
	pub(crate) async fn execute(&self) -> Result<Template> {
		clear_screen()?;
		set_theme(Theme);

		let parachain_config = if self.name.is_none() {
			// If user doesn't select the name guide them to generate a parachain.
			guide_user_to_generate_parachain().await?
		} else {
			self.clone()
		};

		let name = &parachain_config
			.name
			.clone()
			.expect("name can not be none as fallback above is interactive input; qed");
		let provider = &parachain_config.provider.clone().unwrap_or_default();
		let template = match &parachain_config.template {
			Some(template) => template.clone(),
			None => provider.default_template(), // Each provider has a template by default
		};

		is_template_supported(provider, &template)?;
		let config = get_customization_value(
			&template,
			parachain_config.symbol.clone(),
			parachain_config.decimals,
			parachain_config.initial_endowment.clone(),
		)?;

		let tag_version = parachain_config.release_tag.clone();

		generate_parachain_from_template(name, provider, &template, tag_version, config)?;
		Ok(template)
	}
}

async fn guide_user_to_generate_parachain() -> Result<NewParachainCommand> {
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
				"{} {} available option(s) {}",
				provider.description(),
				provider.templates().len(),
				if provider.name() == "Parity" { "[deprecated]" } else { "" }
			),
		);
	}
	let provider = prompt.interact()?;
	let template = display_select_options(provider)?;

	let url = url::Url::parse(&template.repository_url()?).expect("valid repository url");
	// Get only the latest 3 releases
	let latest_3_releases: Vec<Release> = get_latest_3_releases(url).await?;

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
	if template.matches(&Provider::Pop) {
		customizable_options = prompt_customizable_options()?;
	}

	clear_screen()?;

	Ok(NewParachainCommand {
		name: Some(name),
		provider: Some(provider.clone()),
		template: Some(template.clone()),
		release_tag: release_name,
		symbol: Some(customizable_options.symbol),
		decimals: Some(customizable_options.decimals),
		initial_endowment: Some(customizable_options.initial_endowment),
	})
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

	let spinner = cliclack::spinner();
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

fn is_template_supported(provider: &Provider, template: &Template) -> Result<()> {
	if !template.matches(provider) {
		return Err(anyhow::anyhow!(format!(
			"The provider \"{:?}\" doesn't support the {:?} template.",
			provider, template
		)));
	};
	return Ok(());
}

fn display_select_options(provider: &Provider) -> Result<&Template> {
	let mut prompt = cliclack::select("Select the type of parachain:".to_string());
	for (i, template) in provider.templates().into_iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(template);
		}
		prompt = prompt.item(template, template.name(), template.description());
	}
	Ok(prompt.interact()?)
}

fn get_customization_value(
	template: &Template,
	symbol: Option<String>,
	decimals: Option<u8>,
	initial_endowment: Option<String>,
) -> Result<Config> {
	if !matches!(template, Template::Standard)
		&& (symbol.is_some() || decimals.is_some() || initial_endowment.is_some())
	{
		log::warning("Customization options are not available for this template")?;
	}
	return Ok(Config {
		symbol: symbol.clone().expect("default values"),
		decimals: decimals.clone().expect("default values"),
		initial_endowment: initial_endowment.clone().expect("default values"),
	});
}

fn check_destination_path(name_template: &String) -> Result<&Path> {
	let destination_path = Path::new(name_template);
	if destination_path.exists() {
		if !confirm(format!(
			"\"{}\" directory already exists. Would you like to remove it?",
			destination_path.display()
		))
		.interact()?
		{
			outro_cancel(format!(
				"Cannot generate parachain until \"{}\" directory is removed.",
				destination_path.display()
			))?;
			return Err(anyhow::anyhow!(format!(
				"\"{}\" directory already exists.",
				destination_path.display()
			)));
		}
		fs::remove_dir_all(destination_path)?;
	}
	Ok(destination_path)
}

async fn get_latest_3_releases(url: url::Url) -> Result<Vec<Release>> {
	let repo = GitHub::parse(url.as_str())?;
	let mut latest_3_releases: Vec<Release> = repo
		.get_latest_releases()
		.await?
		.into_iter()
		.filter(|r| !r.prerelease)
		.take(3)
		.collect();
	// Get the commit sha for the releases
	for release in latest_3_releases.iter_mut() {
		let commit = repo.get_commit_sha_from_release(&release.tag_name).await?;
		release.commit = Some(commit);
	}
	Ok(latest_3_releases)
}

fn display_release_versions_to_user(releases: Vec<Release>) -> Result<String> {
	let mut prompt = cliclack::select("Select a specific release:".to_string());
	for (i, release) in releases.iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(&release.tag_name);
		}
		prompt = prompt.item(
			&release.tag_name,
			&release.name,
			match &release.commit {
				Some(commit) => format!("{} / {}", &release.tag_name, &commit[..=6]),
				None => release.tag_name.to_string(),
			},
		)
	}
	Ok(prompt.interact()?.to_string())
}

fn prompt_customizable_options() -> Result<Config> {
	let symbol: String = input("What is the symbol of your parachain token?")
		.placeholder("UNIT")
		.default_input("UNIT")
		.interact()?;

	let decimals_input: String = input("How many token decimals?")
		.placeholder("12")
		.default_input("12")
		.interact()?;
	let decimals: u8 = decimals_input.parse::<u8>().expect("input has to be a number");

	let mut initial_endowment: String = input("And the initial endowment for dev accounts?")
		.placeholder("1u64 << 60")
		.default_input("1u64 << 60")
		.interact()?;
	if !is_initial_endowment_valid(&initial_endowment) {
		outro_cancel("⚠️ The specified initial endowment is not valid")?;
		//Prompt the user if want to use the one by default
		if !confirm(format!("📦 Would you like to use the default {}?", DEFAULT_INITIAL_ENDOWMENT))
			.initial_value(true)
			.interact()?
		{
			outro_cancel(
				"🚫 Cannot create a parachain with an incorrect initial endowment value.",
			)?;
			return Err(anyhow::anyhow!("incorrect initial endowment value"));
		}
		initial_endowment = DEFAULT_INITIAL_ENDOWMENT.to_string();
	}
	Ok(Config { symbol, decimals, initial_endowment })
}

#[cfg(test)]
mod tests {

	use super::*;
	use crate::{
		commands::new::{NewArgs, NewCommands::Parachain},
		Cli,
		Commands::New,
	};
	use clap::Parser;
	use git2::Repository;
	use tempfile::tempdir;

	#[tokio::test]
	async fn test_new_parachain_command_with_defaults_executes() -> Result<()> {
		let dir = tempdir()?;
		let cli = Cli::parse_from([
			"pop",
			"new",
			"parachain",
			dir.path().join("test_parachain").to_str().unwrap(),
		]);

		let New(NewArgs { command: Parachain(command) }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		let name = command.name.as_ref().unwrap();
		command.execute().await?;
		// check for git_init
		let repo = Repository::open(Path::new(name))?;
		let reflog = repo.reflog("HEAD")?;
		assert_eq!(reflog.len(), 1);
		Ok(())
	}

	#[tokio::test]
	async fn test_new_parachain_command_execute() -> Result<()> {
		let dir = tempdir()?;
		let command = NewParachainCommand {
			name: Some(dir.path().join("test_parachain").to_str().unwrap().to_string()),
			provider: Some(Provider::Pop),
			template: Some(Template::Standard),
			release_tag: None,
			symbol: Some("UNIT".to_string()),
			decimals: Some(12),
			initial_endowment: Some("1u64 << 60".to_string()),
		};
		command.execute().await?;

		// check for git_init
		let repo = Repository::open(Path::new(&command.name.unwrap()))?;
		let reflog = repo.reflog("HEAD")?;
		assert_eq!(reflog.len(), 1);

		Ok(())
	}

	#[test]
	fn test_is_template_supported() -> Result<()> {
		is_template_supported(&Provider::Pop, &Template::Standard)?;
		assert!(is_template_supported(&Provider::Pop, &Template::ParityContracts).is_err());
		assert!(is_template_supported(&Provider::Pop, &Template::ParityFPT).is_err());

		assert!(is_template_supported(&Provider::Parity, &Template::Standard).is_err());
		is_template_supported(&Provider::Parity, &Template::ParityContracts)?;
		is_template_supported(&Provider::Parity, &Template::ParityFPT)
	}

	#[test]
	fn test_get_customization_values() -> Result<()> {
		let config = get_customization_value(
			&Template::Standard,
			Some("DOT".to_string()),
			Some(6),
			Some("10000".to_string()),
		)?;
		assert_eq!(
			config,
			Config {
				symbol: "DOT".to_string(),
				decimals: 6,
				initial_endowment: "10000".to_string()
			}
		);
		Ok(())
	}
}
