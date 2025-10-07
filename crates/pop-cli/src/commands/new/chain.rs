// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::{self, traits::*},
	common::helpers::check_destination_path,
};
use anyhow::Result;
use clap::{
	Args,
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
};
use console::style;
use pop_chains::{
	ChainTemplate, Config, Provider, instantiate_template_dir, is_initial_endowment_valid,
};
use pop_common::{
	Git, GitHub, Release, enum_variants, enum_variants_without_deprecated,
	templates::{Template, Type},
};
use std::{path::Path, str::FromStr, thread::sleep, time::Duration};
use strum::VariantArray;

const DEFAULT_INITIAL_ENDOWMENT: &str = "1u64 << 60";
const DEFAULT_TOKEN_DECIMALS: &str = "12";
const DEFAULT_TOKEN_SYMBOL: &str = "UNIT";

#[derive(Args, Clone)]
#[cfg_attr(test, derive(Default))]
pub struct NewChainCommand {
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
		help = format!("Template to use. [possible values: {}]", enum_variants_without_deprecated!(ChainTemplate)),
		value_parser = enum_variants!(ChainTemplate),
		hide_possible_values = true // Hide the deprecated templates
	)]
	pub(crate) template: Option<ChainTemplate>,
	#[arg(
		short = 'r',
		long,
		help = "Release tag to use for template. If empty, latest release will be used."
	)]
	pub(crate) release_tag: Option<String>,
	#[arg(long, short, help = "Token Symbol", default_value = DEFAULT_TOKEN_SYMBOL)]
	pub(crate) symbol: Option<String>,
	#[arg(long, short, help = "Token Decimals", default_value = DEFAULT_TOKEN_DECIMALS)]
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

impl NewChainCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<ChainTemplate> {
		// If user doesn't select the name guide them to generate a parachain.
		let parachain_config = if self.name.is_none() {
			guide_user_to_generate_parachain(self.verify, &mut cli::Cli).await?
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

		is_template_supported(provider, &template, &mut cli::Cli)?;
		let config = get_customization_value(
			&template,
			parachain_config.symbol,
			parachain_config.decimals,
			parachain_config.initial_endowment,
			&mut cli::Cli,
		)?;

		let tag_version = parachain_config.release_tag.clone();

		generate_parachain_from_template(
			name,
			provider,
			&template,
			tag_version,
			config,
			self.verify,
			&mut cli::Cli,
		)
		.await?;
		Ok(template)
	}
}

/// Guide the user to generate a parachain from available templates.
async fn guide_user_to_generate_parachain(
	verify: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<NewChainCommand> {
	cli.intro("Generate a parachain")?;

	// Prompt for template selection.
	let provider = {
		let mut prompt = cli.select("Select a template provider: ".to_string());
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
		prompt.interact()?
	};
	let template = display_select_options(provider, cli)?;
	let release_name = choose_release(&template, verify, cli).await?;

	// Prompt for location.
	let name: String = cli
		.input("Where should your project be created?")
		.placeholder("./my-parachain")
		.default_input("./my-parachain")
		.interact()?;

	// Prompt for additional customization options.
	let mut customizable_options = Config {
		symbol: "UNIT".to_string(),
		decimals: 12,
		initial_endowment: "1u64 << 60".to_string(),
	};
	if Provider::Pop.provides(&template) {
		customizable_options = prompt_customizable_options(cli)?;
	}

	Ok(NewChainCommand {
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
	template: &ChainTemplate,
	tag_version: Option<String>,
	config: Config,
	verify: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	cli.intro(format!(
		"Generating \"{name_template}\" using {} from {}!",
		template.name(),
		provider.name()
	))?;

	let destination_path = check_destination_path(Path::new(name_template), cli)?;

	let spinner = cliclack::spinner();
	spinner.start("Generating parachain...");
	let tag = instantiate_template_dir(template, &destination_path, tag_version, config)?;
	if let Err(err) = Git::git_init(&destination_path, "initialized parachain") {
		if err.class() == git2::ErrorClass::Config && err.code() == git2::ErrorCode::NotFound {
			cli.outro_cancel(
				"git signature could not be found. Please configure your git config with your name and email",
			)?;
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
		verify_note = format!(
			" ✅ Fetched the latest release of the template along with its license based on the commit SHA for the release ({}).",
			commit.unwrap_or_default()
		);
	}
	cli.success(format!(
		"Generation complete{}",
		tag.map(|t| format!("\n{}", style(format!("Version: {t} {}", verify_note)).dim()))
			.unwrap_or_default()
	))?;

	if !template.is_audited() {
		// warn about audit status and licensing
		cli.warning(format!("NOTE: the resulting parachain is not guaranteed to be audited or reviewed for security vulnerabilities.\n{}",
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
			"Use `pop up chain -f {network_config}` to launch your parachain on a local network."
		))
	}
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

/// Determines whether the specified template is supported by the provider.
fn is_template_supported(
	provider: &Provider,
	template: &ChainTemplate,
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	if !provider.provides(template) {
		return Err(anyhow::anyhow!(format!(
			"The provider \"{:?}\" doesn't support the {:?} template.",
			provider, template
		)));
	};
	if template.is_deprecated() {
		cli.warning(format!(
			"NOTE: this template is deprecated.{}",
			template.deprecated_message()
		))?;
	}
	Ok(())
}

fn display_select_options(
	provider: &Provider,
	cli: &mut impl cli::traits::Cli,
) -> Result<ChainTemplate> {
	let mut prompt = cli.select("Select the type of parachain:".to_string());
	for (i, template) in provider.templates().into_iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(template);
		}
		prompt = prompt.item(template, template.name(), template.description().trim());
	}
	Ok(prompt.interact()?.clone())
}

fn get_customization_value(
	template: &ChainTemplate,
	symbol: Option<String>,
	decimals: Option<u8>,
	initial_endowment: Option<String>,
	cli: &mut impl cli::traits::Cli,
) -> Result<Config> {
	if !(Provider::Pop.provides(template) || template == &ChainTemplate::ParityGeneric) &&
		(symbol.is_some() || decimals.is_some() || initial_endowment.is_some())
	{
		cli.warning("Customization options are not available for this template")?;
		sleep(Duration::from_secs(3))
	}
	Ok(Config {
		symbol: symbol.unwrap_or_else(|| DEFAULT_TOKEN_SYMBOL.to_string()),
		decimals: decimals
			.unwrap_or_else(|| DEFAULT_TOKEN_DECIMALS.parse::<u8>().expect("default values")),
		initial_endowment: initial_endowment
			.unwrap_or_else(|| DEFAULT_INITIAL_ENDOWMENT.to_string()),
	})
}

/// Gets the latest 3 releases. Prompts the user to choose if releases exist.
/// Otherwise, the default release is used.
///
/// return: `Option<String>` - The release name selected by the user or None if no releases found.
async fn choose_release(
	template: &ChainTemplate,
	verify: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<Option<String>> {
	let url = url::Url::parse(template.repository_url()?).expect("valid repository url");
	let repo = GitHub::parse(url.as_str())?;

	let license = if verify || template.license().is_none() {
		repo.get_repo_license().await?
	} else {
		template.license().unwrap().to_string() // unwrap is safe as it is checked above
	};
	cli.info(format!("Template {}: {}", style("License").bold(), license))?;

	// Get only the latest 3 releases that are supported by the template (default is all)
	let latest_3_releases: Vec<Release> = get_latest_3_releases(&repo, verify)
		.await?
		.into_iter()
		.filter(|r| template.is_supported_version(&r.tag_name))
		.collect();

	let mut release_name = None;
	if !latest_3_releases.is_empty() {
		release_name = Some(display_release_versions_to_user(latest_3_releases, cli)?);
	} else {
		// If supported_versions exists and no other releases are found,
		// then the default branch is not supported and an error is returned
		let _ = template.supported_versions().is_some() &&
			Err(anyhow::anyhow!(
				"No supported versions found for this template. Please open an issue here: https://github.com/r0gue-io/pop-cli/issues "
			))?;

		cli.warning("No releases found for this template. Will use the default branch")?;
	}

	Ok(release_name)
}

async fn get_latest_3_releases(repo: &GitHub, verify: bool) -> Result<Vec<Release>> {
	let mut releases: Vec<Release> = repo.releases(false).await?;
	releases.truncate(3);
	if verify {
		// Get the commit sha for the releases
		for release in releases.iter_mut() {
			let commit = repo.get_commit_sha_from_release(&release.tag_name).await?;
			release.commit = Some(commit);
		}
	}
	Ok(releases)
}

fn display_release_versions_to_user(
	releases: Vec<Release>,
	cli: &mut impl cli::traits::Cli,
) -> Result<String> {
	let mut prompt = cli.select("Select a specific release:".to_string());
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

fn prompt_customizable_options(cli: &mut impl cli::traits::Cli) -> Result<Config> {
	let symbol: String = cli
		.input("What is the symbol of your parachain token?")
		.placeholder(DEFAULT_TOKEN_SYMBOL)
		.default_input(DEFAULT_TOKEN_SYMBOL)
		.interact()?;

	let decimals_input: String = cli
		.input("How many token decimals?")
		.placeholder(DEFAULT_TOKEN_DECIMALS)
		.default_input(DEFAULT_TOKEN_DECIMALS)
		.interact()?;
	let decimals: u8 = decimals_input.parse::<u8>().expect("input has to be a number");

	let mut initial_endowment: String = cli
		.input("And the initial endowment for dev accounts?")
		.placeholder(DEFAULT_INITIAL_ENDOWMENT)
		.default_input(DEFAULT_INITIAL_ENDOWMENT)
		.interact()?;
	if !is_initial_endowment_valid(&initial_endowment) {
		cli.warning("⚠️ The specified initial endowment is not valid")?;
		// Prompt the user if they want to use the one by default
		if !cli
			.confirm(format!("📦 Would you like to use the default {}?", DEFAULT_INITIAL_ENDOWMENT))
			.initial_value(true)
			.interact()?
		{
			cli.outro_cancel(
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
	use cli::MockCli;

	#[test]
	fn display_select_options_works() -> anyhow::Result<()> {
		let mut items_select_template: Vec<(String, String)> = Vec::new();
		for template in Provider::Pop.templates() {
			items_select_template
				.push((template.name().to_string(), template.description().to_string()));
		}
		let mut cli = MockCli::new().expect_select(
			"Select the type of parachain:",
			Some(false),
			true,
			Some(items_select_template),
			1, // "ASSETS"
			None,
		);

		let user_input = display_select_options(&Provider::Pop, &mut cli)?;
		assert_eq!(user_input, ChainTemplate::Assets);

		cli.verify()
	}

	#[test]
	fn test_is_template_supported() -> Result<()> {
		let mut cli = MockCli::new();
		is_template_supported(&Provider::Pop, &ChainTemplate::Standard, &mut cli)?;
		assert!(
			is_template_supported(&Provider::Pop, &ChainTemplate::ParityGeneric, &mut cli).is_err()
		);

		assert!(
			is_template_supported(&Provider::Parity, &ChainTemplate::Standard, &mut cli).is_err()
		);
		is_template_supported(&Provider::Parity, &ChainTemplate::ParityGeneric, &mut cli)
	}

	#[test]
	fn test_get_customization_values() -> Result<()> {
		for template in Provider::Pop.templates() {
			let mut cli = MockCli::new();
			let config = get_customization_value(template, None, None, None, &mut cli)?;
			assert_eq!(
				config,
				Config {
					symbol: "UNIT".to_string(),
					decimals: 12,
					initial_endowment: "1u64 << 60".to_string()
				}
			);
		}
		// For templates that doesn't provide customization options
		let templates: Vec<&ChainTemplate> = Provider::OpenZeppelin
			.templates()
			.into_iter()
			.chain(Provider::Parity.templates())
			.collect();
		for template in templates {
			let mut cli = MockCli::new()
				.expect_warning("Customization options are not available for this template");
			let config =
				get_customization_value(template, Some("DOT".into()), Some(6), None, &mut cli)?;
			assert_eq!(
				config,
				Config {
					symbol: "DOT".to_string(),
					decimals: 6,
					initial_endowment: "1u64 << 60".to_string()
				}
			);
		}
		Ok(())
	}

	#[test]
	fn display_release_versions_to_user_works() -> Result<()> {
		let releases: Vec<Release> = vec![
			Release {
				tag_name: "polkadot-v.1.14.0".into(),
				name: "Polkadot v1.14".into(),
				prerelease: false,
				commit: Some("4a6e8ef5cade26e0da1fe74ab8bf3509d7f99d59".into()),
				published_at: "2025-01-01T00:00:00Z".into(),
			},
			Release {
				tag_name: "polkadot-v.1.13.0".into(),
				name: "Polkadot v1.13".into(),
				prerelease: false,
				commit: Some("e504836b1165bd19ab446215103cb1ecbe1a23df".into()),
				published_at: "2024-01-01T00:00:00Z".into(),
			},
			Release {
				tag_name: "polkadot-v.1.12.0".into(),
				name: "Polkadot v1.12".into(),
				prerelease: false,
				commit: Some("85d97816d195508d9a684e3e1e63f82bfbb41eb5".into()),
				published_at: "2023-01-01T00:00:00Z".into(),
			},
		];
		let mut cli = MockCli::new().expect_select(
			"Select a specific release:",
			Some(false),
			true,
			Some(vec![
				(
					"Polkadot v1.14".into(),
					format!(
						"{} / {}",
						"polkadot-v.1.14.0",
						&"4a6e8ef5cade26e0da1fe74ab8bf3509d7f99d59".to_string()[..=6]
					),
				),
				(
					"Polkadot v1.13".into(),
					format!(
						"{} / {}",
						"polkadot-v.1.13.0",
						&"e504836b1165bd19ab446215103cb1ecbe1a23df".to_string()[..=6]
					),
				),
				(
					"Polkadot v1.12".into(),
					format!(
						"{} / {}",
						"polkadot-v.1.12.0",
						&"85d97816d195508d9a684e3e1e63f82bfbb41eb5".to_string()[..=6]
					),
				),
			]),
			0, // "Polkadot v1.14"
			None,
		);
		assert_eq!(display_release_versions_to_user(releases, &mut cli)?, "polkadot-v.1.14.0");
		Ok(())
	}

	#[test]
	fn get_prompt_customizable_options_fails_wrong_endowment() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_input("What is the symbol of your parachain token?", "DOT".into())
			.expect_input("How many token decimals?", "6".into())
			.expect_input("And the initial endowment for dev accounts?", "10_000".into())
			.expect_warning("⚠️ The specified initial endowment is not valid")
			.expect_confirm("📦 Would you like to use the default 1u64 << 60?", false)
			.expect_outro_cancel(
				"🚫 Cannot create a parachain with an incorrect initial endowment value.",
			);
		assert!(matches!(
			prompt_customizable_options(&mut cli),
			anyhow::Result::Err(message) if message.to_string() == "incorrect initial endowment value"
		));
		Ok(())
	}
}
