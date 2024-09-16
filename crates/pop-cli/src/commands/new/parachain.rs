// SPDX-License-Identifier: GPL-3.0

use crate::cli::{self, traits::*};
use anyhow::Result;
use clap::{
	builder::{PossibleValue, PossibleValuesParser, TypedValueParser},
	Args,
};
use console::style;
use pop_common::{
	enum_variants,
	templates::{Template, Type},
	Git, GitHub, Release,
};
use pop_parachains::{
	instantiate_template_dir, is_initial_endowment_valid, Config, Parachain, Provider,
};
use std::{
	fs,
	path::{Path, PathBuf},
	str::FromStr,
	thread::sleep,
	time::Duration,
};
use strum::VariantArray;

const DEFAULT_INITIAL_ENDOWMENT: &str = "1u64 << 60";
const DEFAULT_TOKEN_DECIMALS: &str = "12";
const DEFAULT_TOKEN_SYMBOL: &str = "UNIT";

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
		help = "Template to use.",
		value_parser = enum_variants!(Parachain)
	)]
	pub(crate) template: Option<Parachain>,
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

impl NewParachainCommand {
	/// Executes the command.
	pub(crate) async fn execute(self) -> Result<Parachain> {
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

		is_template_supported(provider, &template)?;
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
) -> Result<NewParachainCommand> {
	cli.intro("Generate a parachain")?;

	// Prompt for template selection.
	let provider = {
		let mut prompt = cli.select("Select a template provider:".to_string());
		for (i, provider) in Provider::types().iter().enumerate() {
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
		prompt.interact()?
	};
	let template = {
		let mut prompt = cli.select("Select the type of parachain:".to_string());
		for (i, template) in provider.templates().into_iter().enumerate() {
			if i == 0 {
				prompt = prompt.initial_value(template);
			}
			prompt = prompt.item(template, template.name(), template.description());
		}
		prompt.interact()?
	};
	let release_name = choose_release(template, verify, cli).await?;

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
	cli: &mut impl cli::traits::Cli,
) -> Result<()> {
	cli.intro(format!(
		"Generating \"{name_template}\" using {} from {}!",
		template.name(),
		provider.name()
	))?;

	let destination_path = check_destination_path(name_template, cli)?;

	let spinner = cliclack::spinner();
	spinner.start("Generating parachain...");
	let tag = instantiate_template_dir(template, &destination_path, tag_version, config)?;
	if let Err(err) = Git::git_init(&destination_path, "initialized parachain") {
		if err.class() == git2::ErrorClass::Config && err.code() == git2::ErrorCode::NotFound {
			cli.outro_cancel("git signature could not be found. Please configure your git config with your name and email")?;
		}
	}
	spinner.clear();

	// Replace spinner with success.
	console::Term::stderr().clear_last_lines(2)?;
	let mut verify_note = "".to_string();
	if verify && tag.is_some() {
		let url = url::Url::parse(&template.repository_url()?).expect("valid repository url");
		let repo = GitHub::parse(url.as_str())?;
		let commit = repo.get_commit_sha_from_release(&tag.clone().unwrap()).await;
		verify_note = format!(" ✅ Fetched the latest release of the template along with its license based on the commit SHA for the release ({}).", commit.unwrap_or_default());
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
		"Use `pop build` to build your parachain.".into(),
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
	cli.success(format!("Next Steps:\n{}", next_steps.join("\n")))?;

	cli.outro(format!(
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
	return Ok(());
}

fn get_customization_value(
	template: &Parachain,
	symbol: Option<String>,
	decimals: Option<u8>,
	initial_endowment: Option<String>,
	cli: &mut impl cli::traits::Cli,
) -> Result<Config> {
	if Provider::Pop.provides(&template)
		&& (symbol.is_some() || decimals.is_some() || initial_endowment.is_some())
	{
		cli.warning("Customization options are not available for this template")?;
		sleep(Duration::from_secs(3))
	}
	return Ok(Config {
		symbol: symbol.unwrap_or_else(|| DEFAULT_TOKEN_SYMBOL.to_string()),
		decimals: decimals
			.unwrap_or_else(|| DEFAULT_TOKEN_DECIMALS.parse::<u8>().expect("default values")),
		initial_endowment: initial_endowment
			.unwrap_or_else(|| DEFAULT_INITIAL_ENDOWMENT.to_string()),
	});
}

fn check_destination_path(
	name_template: &String,
	cli: &mut impl cli::traits::Cli,
) -> Result<PathBuf> {
	let destination_path = Path::new(name_template);
	if destination_path.exists() {
		if !cli
			.confirm(format!(
				"\"{}\" directory already exists. Would you like to remove it?",
				destination_path.display()
			))
			.interact()?
		{
			cli.outro_cancel(format!(
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
	Ok(destination_path.to_path_buf())
}

/// Gets the latest 3 releases. Prompts the user to choose if releases exist.
/// Otherwise, the default release is used.
///
/// return: `Option<String>` - The release name selected by the user or None if no releases found.
async fn choose_release(
	template: &Parachain,
	verify: bool,
	cli: &mut impl cli::traits::Cli,
) -> Result<Option<String>> {
	let url = url::Url::parse(&template.repository_url()?).expect("valid repository url");
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
	if latest_3_releases.len() > 0 {
		release_name = Some(display_release_versions_to_user(latest_3_releases, cli)?);
	} else {
		// If supported_versions exists and no other releases are found,
		// then the default branch is not supported and an error is returned
		let _ = template.supported_versions().is_some()
			&& Err(anyhow::anyhow!(
				"No supported versions found for this template. Please open an issue here: https://github.com/r0gue-io/pop-cli/issues "
			))?;

		cli.warning("No releases found for this template. Will use the default branch")?;
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
		cli.outro_cancel("⚠️ The specified initial endowment is not valid")?;
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
	use crate::{
		commands::new::{Command::Parachain as ParachainCommand, NewArgs},
		Cli,
		Command::New,
	};
	use clap::Parser;
	use cli::MockCli;
	use git2::Repository;
	use tempfile::tempdir;

	#[tokio::test]
	async fn new_parachain_command_with_defaults_executes_works() -> Result<()> {
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
	async fn guide_user_to_generate_parachain_works() -> anyhow::Result<()> {
		let mut items_select_provider: Vec<(String, String)> = Vec::new();
		for provider in Provider::VARIANTS {
			items_select_provider.push((
				provider.name().to_string(),
				format!(
					"{} {} available option(s) {}",
					provider.description(),
					provider.templates().len(),
					if provider.name() == "Parity" { "[deprecated]" } else { "" }
				),
			));
		}
		let mut items_select_template: Vec<(String, String)> = Vec::new();
		for template in Provider::Pop.templates() {
			items_select_template
				.push((template.name().to_string(), template.description().to_string()));
		}
		let mut cli = MockCli::new()
			.expect_intro("Generate a parachain")
			.expect_select::<&str>(
				"Select a specific release:",
				Some(false),
				true,
				None, // We don't care about the values here (release list change each time)
				1,
			)
			.expect_select::<Parachain>(
				"Select the type of parachain:",
				Some(false),
				true,
				Some(items_select_template),
				2, // "ASSETS"
			)
			.expect_select::<Provider>(
				"Select a template provider:",
				Some(false),
				true,
				Some(items_select_provider.clone()),
				1, // "POP"
			)
			.expect_info(format!("Template {}: Unlicense", style("License").bold()))
			.expect_input(
				"And the initial endowment for dev accounts?",
				DEFAULT_INITIAL_ENDOWMENT.into(),
			)
			.expect_input("How many token decimals?", "6".into())
			.expect_input("What is the symbol of your parachain token?", "DOT".into())
			.expect_input("Where should your project be created?", "./assets-parachain".into());

		let user_input = guide_user_to_generate_parachain(false, &mut cli).await?;
		assert_eq!(user_input.name, Some("./assets-parachain".into()));
		assert_eq!(user_input.provider, Some(Provider::Pop));
		assert_eq!(user_input.template, Some(Parachain::Assets));
		assert_eq!(user_input.symbol, Some("DOT".into()));
		assert_eq!(user_input.decimals, Some(6));
		assert_eq!(user_input.initial_endowment, Some(DEFAULT_INITIAL_ENDOWMENT.into()));

		cli.verify()?;
		Ok(())
	}

	#[tokio::test]
	async fn generate_parachain_from_template_works() -> anyhow::Result<()> {
		let dir = tempdir()?;
		let parachain_path = dir.path().join("my-parachain");
		let next_steps: Vec<_> = vec![
			format!("cd into {:?} and enjoy hacking! 🚀", parachain_path.display()),
			"Use `pop build` to build your parachain.".into(),
			format!(
				"Use `pop up parachain -f ./network.toml` to launch your parachain on a local network."
			),
		]
		.iter()
		.map(|s| style(format!("{} {s}", console::Emoji("●", ">"))).dim().to_string())
		.collect();
		let mut cli = MockCli::new()
			.expect_intro(format!(
				"Generating \"{}\" using Assets from Pop!",
				parachain_path.display().to_string()
			))
			.expect_success(format!(
				"Generation complete{}",
				format!("\n{}", style(format!("Version: polkadot-v1.11.0 ")).dim())
			))
			.expect_warning(format!("NOTE: the resulting parachain is not guaranteed to be audited or reviewed for security vulnerabilities.\n{}",
				style(format!("Please consult the source repository at {} to assess production suitability and licensing restrictions.", &Parachain::Assets.repository_url()?))
				.dim()))
			.expect_success(format!("Next Steps:\n{}", next_steps.join("\n")))
			.expect_outro(format!(
				"Need help? Learn more at {}\n",
				style("https://learn.onpop.io").magenta().underlined()
			));
		generate_parachain_from_template(
			&parachain_path.display().to_string(),
			&Provider::Pop,
			&Parachain::Assets,
			Some("polkadot-v1.11.0".into()),
			Config {
				symbol: "DOT".to_string(),
				decimals: 6,
				initial_endowment: "1u64 << 60".to_string(),
			},
			false,
			&mut cli,
		)
		.await?;
		cli.verify()?;
		Ok(())
	}

	#[test]
	fn is_template_supported_works() -> Result<()> {
		is_template_supported(&Provider::Pop, &Parachain::Standard)?;
		assert!(is_template_supported(&Provider::Pop, &Parachain::ParityContracts).is_err());
		assert!(is_template_supported(&Provider::Pop, &Parachain::ParityFPT).is_err());

		assert!(is_template_supported(&Provider::Parity, &Parachain::Standard).is_err());
		is_template_supported(&Provider::Parity, &Parachain::ParityContracts)?;
		is_template_supported(&Provider::Parity, &Parachain::ParityFPT)
	}

	#[test]
	fn get_customization_values_works() -> Result<()> {
		for template in Provider::Pop.templates() {
			let mut cli = MockCli::new();
			let config = get_customization_value(&template, None, None, None, &mut cli)?;
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
		let templates: Vec<&Parachain> = Provider::OpenZeppelin
			.templates()
			.into_iter()
			.chain(Provider::Parity.templates().into_iter())
			.collect();
		for template in templates {
			let mut cli = MockCli::new()
				.expect_warning("Customization options are not available for this template");
			let config =
				get_customization_value(&template, Some("DOT".into()), Some(6), None, &mut cli)?;
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
	fn check_destination_path_works() -> anyhow::Result<()> {
		let dir = tempdir()?;
		let name_template = format!("{}/test-parachain", dir.path().display());
		let parachain_path = dir.path().join(&name_template);
		let mut cli = MockCli::new();
		// directory doesn't exist
		let output_path = check_destination_path(&name_template, &mut cli)?;
		assert_eq!(output_path, parachain_path);
		// directory already exists and user confirms to remove it
		fs::create_dir(parachain_path.as_path())?;
		let mut cli = MockCli::new().expect_confirm(
			format!(
				"\"{}\" directory already exists. Would you like to remove it?",
				parachain_path.display().to_string()
			),
			true,
		);
		let output_path = check_destination_path(&name_template, &mut cli)?;
		assert_eq!(output_path, parachain_path);
		assert!(!parachain_path.exists());
		// directory already exists and user confirms to not remove it
		fs::create_dir(parachain_path.as_path())?;
		let mut cli = MockCli::new()
			.expect_confirm(
				format!(
					"\"{}\" directory already exists. Would you like to remove it?",
					parachain_path.display().to_string()
				),
				false,
			)
			.expect_outro_cancel(format!(
				"Cannot generate parachain until \"{}\" directory is removed.",
				parachain_path.display()
			));

		assert!(matches!(
			check_destination_path(&name_template, &mut cli),
			anyhow::Result::Err(message) if message.to_string() == format!(
				"\"{}\" directory already exists.",
				parachain_path.display().to_string()
			)
		));

		cli.verify()?;
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
			},
			Release {
				tag_name: "polkadot-v.1.13.0".into(),
				name: "Polkadot v1.13".into(),
				prerelease: false,
				commit: Some("e504836b1165bd19ab446215103cb1ecbe1a23df".into()),
			},
			Release {
				tag_name: "polkadot-v.1.12.0".into(),
				name: "Polkadot v1.12".into(),
				prerelease: false,
				commit: Some("85d97816d195508d9a684e3e1e63f82bfbb41eb5".into()),
			},
		];
		let mut cli = MockCli::new().expect_select::<&str>(
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
		);
		assert_eq!(display_release_versions_to_user(releases, &mut cli)?, "polkadot-v.1.14.0");
		Ok(())
	}

	#[test]
	fn get_prompt_customizable_options_fails_wrong_endowment() -> Result<()> {
		let mut cli = MockCli::new()
			.expect_input("And the initial endowment for dev accounts?", "10_000".into())
			.expect_input("How many token decimals?", "6".into())
			.expect_input("What is the symbol of your parachain token?", "DOT".into())
			.expect_outro_cancel(
				"🚫 Cannot create a parachain with an incorrect initial endowment value.",
			)
			.expect_confirm("📦 Would you like to use the default 1u64 << 60?", false)
			.expect_outro_cancel("⚠️ The specified initial endowment is not valid");
		assert!(matches!(
			prompt_customizable_options(&mut cli),
			anyhow::Result::Err(message) if message.to_string() == "incorrect initial endowment value"
		));
		Ok(())
	}
}
