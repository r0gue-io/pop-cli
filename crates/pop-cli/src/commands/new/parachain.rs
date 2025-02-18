// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{traits::Cli as _, Cli},
	style::style,
};
use anyhow::Result;
use clap::{
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
	Args,
};
use cliclack::{
	confirm, input,
	log::{self, success, warning},
	outro, outro_cancel,
};
use pop_common::{
	enum_variants, enum_variants_without_deprecated,
	templates::{Template, Type},
	Git, GitHub, Release,
};
use pop_parachains::{
	instantiate_template_dir, is_initial_endowment_valid, Config, Parachain, Provider,
};
use std::{fs, path::Path, str::FromStr, thread::sleep, time::Duration};
use strum::VariantArray;

const DEFAULT_INITIAL_ENDOWMENT: &str = "1u64 << 60";

#[derive(Args, Clone)]
pub struct NewParachainCommand {
	#[arg(help = "Name of the project. If empty assistance in the process will be provided.")]
	pub(crate) name: Option<String>,
	#[arg(
		help = "Template provider.",
		default_value = Provider::Pop.as_ref(),
		value_parser = enum_variants!(Provider)
	)]
	pub(crate) provider: Option<Provider>,
	#[arg(
		short = 't',
		long,
		help = format!("Template to use. [possible values: {}]", enum_variants_without_deprecated!(Parachain)),
		value_parser = enum_variants!(Parachain),
		hide_possible_values = true // Hide the deprecated templates
	)]
	pub(crate) template: Option<Parachain>,
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
	#[arg(
		short = 'v',
		long,
		help = "Verifies the commit SHA when fetching the latest license and release from GitHub."
	)]
	pub(crate) verify: bool,
}

impl NewParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<Parachain> {
		// If user doesn't select the name guide them to generate a parachain.
		let parachain_config = if self.name.is_none() {
			guide_user_to_generate_parachain(self.verify).await?
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
			None => provider.default_template().expect("parachain templates have defaults; qed."), /* Each provider has a template by default */
		};

		is_template_supported(provider, &template)?;
		let config = get_customization_value(
			&template,
			parachain_config.symbol.clone(),
			parachain_config.decimals,
			parachain_config.initial_endowment.clone(),
		)?;

		let tag_version = parachain_config.release_tag.clone();

		generate_parachain_from_template(
			name,
			provider,
			&template,
			tag_version,
			config,
			self.verify,
		)
		.await?;
		Ok(template)
	}
}

/// Guide the user to generate a parachain from available templates.
async fn guide_user_to_generate_parachain(verify: bool) -> Result<NewParachainCommand> {
	Cli.intro("Generate a parachain")?;

	// Prompt for template selection.
	let mut prompt = cliclack::select("Select a template provider: ".to_string());
	for (i, provider) in Provider::types().iter().enumerate() {
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
	let release_name = choose_release(template, verify).await?;

	// Prompt for location.
	let name: String = input("Where should your project be created?")
		.placeholder("./my-parachain")
		.default_input("./my-parachain")
		.interact()?;

	// Prompt for additional customization options.
	let mut customizable_options = Config {
		symbol: "UNIT".to_string(),
		decimals: 12,
		initial_endowment: "1u64 << 60".to_string(),
	};
	if Provider::Pop.provides(template) {
		customizable_options = prompt_customizable_options()?;
	}

	Ok(NewParachainCommand {
		name: Some(name),
		provider: Some(provider.clone()),
		template: Some(template.clone()),
		release_tag: release_name,
		symbol: Some(customizable_options.symbol),
		decimals: Some(customizable_options.decimals),
		initial_endowment: Some(customizable_options.initial_endowment),
		verify,
	})
}

async fn generate_parachain_from_template(
	name_template: &String,
	provider: &Provider,
	template: &Parachain,
	tag_version: Option<String>,
	config: Config,
	verify: bool,
) -> Result<()> {
	Cli.intro(format!(
		"Generating \"{name_template}\" using {} from {}!",
		template.name(),
		provider.name()
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
	spinner.clear();

	// Replace spinner with success.
	console::Term::stderr().clear_last_lines(2)?;
	let mut verify_note = "".to_string();
	if verify && tag.is_some() {
		let url = url::Url::parse(template.repository_url()?).expect("valid repository url");
		let repo = GitHub::parse(url.as_str())?;
		let commit = repo.get_commit_sha_from_release(&tag.clone().unwrap()).await;
		verify_note = format!(" ✅ Fetched the latest release of the template along with its license based on the commit SHA for the release ({}).", commit.unwrap_or_default());
	}
	success(format!(
		"Generation complete{}",
		tag.map(|t| format!("\n{}", style(format!("Version: {t} {}", verify_note)).dim()))
			.unwrap_or_default()
	))?;

	if !template.is_audited() {
		// warn about audit status and licensing
		warning(format!("NOTE: the resulting parachain is not guaranteed to be audited or reviewed for security vulnerabilities.\n{}",
						style(format!("Please consult the source repository at {} to assess production suitability and licensing restrictions.", template.repository_url()?))
							.dim()))?;
	}

	// add next steps
	let mut next_steps = vec![
		format!("cd into \"{name_template}\" and enjoy hacking! 🚀"),
		"Use `pop build --release` to build your parachain.".into(),
	];
	if let Some(network_config) = template.network_config() {
		next_steps.push(format!(
			"Use `pop up parachain -f {network_config}` to launch your parachain on a local network."
		))
	}
	let next_steps: Vec<_> = next_steps
		.iter()
		.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
		.collect();
	success(format!("Next Steps:\n{}", next_steps.join("\n")))?;

	outro(format!(
		"Need help? Learn more at {}\n",
		style("https://learn.onpop.io").magenta().underlined()
	))?;
	Ok(())
}

/// Determines whether the specified template is supported by the provider.
fn is_template_supported(provider: &Provider, template: &Parachain) -> Result<()> {
	if !provider.provides(template) {
		return Err(anyhow::anyhow!(format!(
			"The provider \"{:?}\" doesn't support the {:?} template.",
			provider, template
		)));
	};
	if template.is_deprecated() {
		warning(format!("NOTE: this template is deprecated.{}", template.deprecated_message()))?;
	}
	Ok(())
}

fn display_select_options(provider: &Provider) -> Result<&Parachain> {
	let mut prompt = cliclack::select("Select the type of parachain:".to_string());
	for (i, template) in provider.templates().into_iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(template);
		}
		prompt = prompt.item(template, template.name(), template.description().trim());
	}
	Ok(prompt.interact()?)
}

fn get_customization_value(
	template: &Parachain,
	symbol: Option<String>,
	decimals: Option<u8>,
	initial_endowment: Option<String>,
) -> Result<Config> {
	if !matches!(template, Parachain::Standard) &&
		(symbol.is_some() || decimals.is_some() || initial_endowment.is_some())
	{
		log::warning("Customization options are not available for this template")?;
		sleep(Duration::from_secs(3))
	}
	Ok(Config {
		symbol: symbol.clone().expect("default values"),
		decimals: decimals.expect("default values"),
		initial_endowment: initial_endowment.clone().expect("default values"),
	})
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

/// Gets the latest 3 releases. Prompts the user to choose if releases exist.
/// Otherwise, the default release is used.
///
/// return: `Option<String>` - The release name selected by the user or None if no releases found.
async fn choose_release(template: &Parachain, verify: bool) -> Result<Option<String>> {
	let url = url::Url::parse(template.repository_url()?).expect("valid repository url");
	let repo = GitHub::parse(url.as_str())?;

	let license = if verify || template.license().is_none() {
		repo.get_repo_license().await?
	} else {
		template.license().unwrap().to_string() // unwrap is safe as it is checked above
	};
	log::info(format!("Template {}: {}", style("License").bold(), license))?;

	// Get only the latest 3 releases that are supported by the template (default is all)
	let latest_3_releases: Vec<Release> = get_latest_3_releases(&repo, verify)
		.await?
		.into_iter()
		.filter(|r| template.is_supported_version(&r.tag_name))
		.collect();

	let mut release_name = None;
	if !latest_3_releases.is_empty() {
		release_name = Some(display_release_versions_to_user(latest_3_releases)?);
	} else {
		// If supported_versions exists and no other releases are found,
		// then the default branch is not supported and an error is returned
		let _ = template.supported_versions().is_some()
			&& Err(anyhow::anyhow!(
				"No supported versions found for this template. Please open an issue here: https://github.com/r0gue-io/pop-cli/issues "
			))?;

		warning("No releases found for this template. Will use the default branch")?;
	}

	Ok(release_name)
}

async fn get_latest_3_releases(repo: &GitHub, verify: bool) -> Result<Vec<Release>> {
	let mut latest_3_releases: Vec<Release> =
		repo.releases().await?.into_iter().filter(|r| !r.prerelease).take(3).collect();
	if verify {
		// Get the commit sha for the releases
		for release in latest_3_releases.iter_mut() {
			let commit = repo.get_commit_sha_from_release(&release.tag_name).await?;
			release.commit = Some(commit);
		}
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
		// Prompt the user if they want to use the one by default
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
		commands::new::{Command::Parachain as ParachainCommand, NewArgs},
		Cli,
		Command::New,
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

		let New(NewArgs { command: ParachainCommand(command) }) = cli.command else {
			panic!("unable to parse command")
		};
		// Execute
		let name = command.name.clone().unwrap();
		command.execute().await?;
		// check for git_init
		let repo = Repository::open(Path::new(&name))?;
		let reflog = repo.reflog("HEAD")?;
		assert_eq!(reflog.len(), 1);
		Ok(())
	}

	#[tokio::test]
	async fn test_new_parachain_command_execute() -> Result<()> {
		let dir = tempdir()?;
		let name = dir.path().join("test_parachain").to_str().unwrap().to_string();
		let command = NewParachainCommand {
			name: Some(name.clone()),
			provider: Some(Provider::Pop),
			template: Some(Parachain::Standard),
			release_tag: None,
			symbol: Some("UNIT".to_string()),
			decimals: Some(12),
			initial_endowment: Some("1u64 << 60".to_string()),
			verify: false,
		};
		command.execute().await?;

		// check for git_init
		let repo = Repository::open(Path::new(&name))?;
		let reflog = repo.reflog("HEAD")?;
		assert_eq!(reflog.len(), 1);

		Ok(())
	}

	#[test]
	fn test_is_template_supported() -> Result<()> {
		is_template_supported(&Provider::Pop, &Parachain::Standard)?;
		assert!(is_template_supported(&Provider::Pop, &Parachain::ParityContracts).is_err());
		assert!(is_template_supported(&Provider::Pop, &Parachain::ParityGeneric).is_err());

		assert!(is_template_supported(&Provider::Parity, &Parachain::Standard).is_err());
		is_template_supported(&Provider::Parity, &Parachain::ParityContracts)?;
		is_template_supported(&Provider::Parity, &Parachain::ParityGeneric)
	}

	#[test]
	fn test_get_customization_values() -> Result<()> {
		let config = get_customization_value(
			&Parachain::Standard,
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
