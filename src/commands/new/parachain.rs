use crate::{
	engines::parachain_engine::{instantiate_template_dir, Config},
	git::{GitHub, TagInfo},
	helpers::git_init,
	style::{style, Theme},
};
use anyhow::Result;
use clap::{Args, Parser};
use std::{fs, path::Path};
use strum_macros::{Display, EnumString};
use url::Url;

use cliclack::{clear_screen, confirm, input, intro, outro, outro_cancel, set_theme};

#[derive(Clone, Parser, Debug, Display, EnumString, PartialEq)]
pub enum Template {
	#[strum(serialize = "Pop Base Parachain Template", serialize = "base")]
	Base,
	#[strum(serialize = "OpenZeppeling Runtime Parachain Template", serialize = "template")]
	OZTemplate,
	#[strum(serialize = "Parity Contracts Node Template", serialize = "cpt")]
	ParityContracts,
	#[strum(serialize = "Parity Frontier Parachain Template", serialize = "fpt")]
	ParityFPT,
}
impl Template {
	fn is_provider_correct(&self, provider: &Provider) -> bool {
		match provider {
			Provider::Pop => return self == &Template::Base,
			Provider::OpenZeppelin => return self == &Template::OZTemplate,
			Provider::Parity => {
				return self == &Template::ParityContracts || self == &Template::ParityFPT
			},
		}
	}
	fn from(provider_name: &str) -> Self {
		match provider_name {
			"base" => Template::Base,
			"template" => Template::OZTemplate,
			"cpt" => Template::ParityContracts,
			"fpt" => Template::ParityFPT,
			_ => Template::Base,
		}
	}
	fn repository(&self) -> Url {
		match &self {
			Template::Base => Url::parse("https://github.com/r0gue-io/base-parachain")
				.expect("valid pop base template repository url"),
			Template::OZTemplate => {
				Url::parse("https://github.com/OpenZeppelin/polkadot-runtime-template")
					.expect("valid openzeppelin template repository url")
			},
			Template::ParityContracts => {
				Url::parse("https://github.com/paritytech/substrate-contracts-node")
					.expect("valid parity substrate contract repository url")
			},
			Template::ParityFPT => {
				Url::parse("https://github.com/paritytech/frontier-parachain-template")
					.expect("valid partity frontier repository url")
			},
		}
	}
}

#[derive(Clone, Default, Parser, Debug, Display, EnumString, PartialEq)]
pub enum Provider {
	#[default]
	#[strum(serialize = "Pop", serialize = "pop")]
	Pop,
	#[strum(serialize = "OpenZeppelin", serialize = "openzeppelin")]
	OpenZeppelin,
	#[strum(serialize = "Parity", serialize = "parity")]
	Parity,
}
impl Provider {
	fn default_template(&self) -> Template {
		match &self {
			Provider::Pop => return Template::Base,
			Provider::OpenZeppelin => return Template::OZTemplate,
			Provider::Parity => return Template::ParityContracts,
		}
	}
	fn from(provider_name: &str) -> Self {
		match provider_name {
			"Pop" => Provider::Pop,
			"OpenZeppelin" => Provider::OpenZeppelin,
			"Parity" => Provider::Parity,
			_ => Provider::Pop,
		}
	}
	fn display_select_options(&self) -> &str {
		match &self {
			Provider::Pop => {
				return cliclack::select(format!("Select a template provider: "))
					.initial_value("base")
					.item("base", "Base Parachain", "A standard parachain")
					.interact()
					.expect("Error parsing user input");
			},
			Provider::OpenZeppelin => {
				return cliclack::select(format!("Select a template provider: "))
					.initial_value("template")
					.item(
						"template",
						"OpenZeppeling Template",
						"OpenZeppeling Runtime Parachain Template",
					)
					.interact()
					.expect("Error parsing user input");
			},
			Provider::Parity => {
				return cliclack::select(format!("Select a template provider: "))
					.initial_value("cpt")
					.item("cpt", "Parity Contracts", "A parachain supporting WebAssembly smart contracts such as ink!.")
					.item("fpt", "Parity EVM", "A parachain supporting smart contracts targeting the Ethereum Virtual Machine (EVM).")
					.interact()
					.expect("Error parsing user input");
			},
		};
	}
}

#[derive(Args)]
pub struct NewParachainCommand {
	#[arg(help = "Name of the project. Also works as a directory path for your project")]
	pub(crate) name: Option<String>,
	#[arg(help = "Provider to pick template: Options are pop, openzeppelin and parity.")]
	#[arg(default_value = "pop")]
	pub(crate) provider: Option<Provider>,
	#[arg(
		help = "Template to use. Options are base for Pop, template for OpenZeppelin and cpt and fpt for Parity templates"
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
}

impl NewParachainCommand {
	pub(crate) async fn execute(&self) -> Result<()> {
		clear_screen()?;
		if let Some(name_template) = &self.name {
			let provider = &self.provider.clone().unwrap_or(Provider::Pop);
			let template = &self.template.clone().unwrap_or(provider.default_template());
			if !template.is_provider_correct(provider) {
				outro_cancel(format!(
					"The provider \"{}\" doesn't support the {} template.",
					provider, template
				))?;
				return Ok(());
			};
			let config = Config {
				symbol: self.symbol.clone().expect("default values"),
				decimals: self.decimals.clone().expect("default values"),
				initial_endowment: self.initial_endowment.clone().expect("default values"),
			};
			generate_template(name_template, provider, template, config)?
		} else {
			guide_user().await?;
		}
		Ok(())
	}
}

async fn guide_user() -> Result<()> {
	let name: String = input("Where should your project be created?")
		.placeholder("my-parachain")
		.interact()?;

	let provider_name = cliclack::select(format!("Select a template provider: "))
		.initial_value("Pop")
		.item("Pop", "Pop", "An all-in-one tool for Polkadot development. 1 available options")
		.item(
			"OpenZeppelin",
			"OpenZeppelin",
			"The standard for secure blockchain applications. 1 available options",
		)
		.item("Parity", "Parity", "Solutions for a trust-free world. 2 available options")
		.interact()?;

	let provider = Provider::from(provider_name);
	let template_name = provider.display_select_options();
	let template = Template::from(template_name);

	// Get the releases
	let url = template.repository();
	let latest_3_releases = GitHub::get_latest_releases(3, &url).await?;

	let version = display_versions(latest_3_releases)?;
	//println!("{:?}", version);

	generate_template(
		&name,
		&provider,
		&template,
		Config {
			symbol: "UNIT".to_string(),
			decimals: 12,
			initial_endowment: "1u64 << 60".to_string(),
		},
	)
	Ok(())
}

fn generate_template(
	name_template: &String,
	provider: &Provider,
	template: &Template,
	version: Option<String>,
	config: Config,
) -> Result<()> {
	intro(format!(
		"{}: Generating \"{}\" using {} from {}!",
		style(" Pop CLI ").black().on_magenta(),
		name_template,
		template,
		provider
	))?;
	set_theme(Theme);
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
			return Ok(());
		}
		fs::remove_dir_all(destination_path)?;
	}
	let mut spinner = cliclack::spinner();
	spinner.start("Generating parachain...");
	instantiate_template_dir(template, destination_path, config)?;
	if let Err(err) = git_init(destination_path, "initialized parachain") {
		if err.class() == git2::ErrorClass::Config && err.code() == git2::ErrorCode::NotFound {
			outro_cancel("git signature could not be found. Please configure your git config with your name and email")?;
		}
	}
	spinner.stop("Generation complete");
	outro(format!("cd into \"{}\" and enjoy hacking! 🚀", name_template))?;
	Ok(())
}

fn display_versions(latest_3_releases: Vec<TagInfo>) -> Result<String> {
	let version;
	if latest_3_releases.len() == 3 {
		version = cliclack::select(format!("Select a template provider: "))
			.initial_value(&latest_3_releases[0].tag_name)
			.item(
				&latest_3_releases[0].tag_name,
				&latest_3_releases[0].name,
				format!("{} ({})", &latest_3_releases[0].tag_name, &latest_3_releases[0].id)
			)
			.item(
				&latest_3_releases[1].tag_name,
				&latest_3_releases[1].name,
				format!("{} ({})", &latest_3_releases[1].tag_name, &latest_3_releases[1].id)
			)
			.item(
				&latest_3_releases[2].tag_name,
				&latest_3_releases[2].name,
				format!("{} ({})", &latest_3_releases[2].tag_name, &latest_3_releases[2].id)
			)
			.interact()?;
	} else if latest_3_releases.len() == 2 {
		version = cliclack::select(format!("Select a template provider: "))
			.initial_value(&latest_3_releases[0].tag_name)
			.item(
				&latest_3_releases[0].tag_name,
				&latest_3_releases[0].name,
				format!("{} ({})", &latest_3_releases[0].tag_name, &latest_3_releases[0].id),
			)
			.item(
				&latest_3_releases[1].tag_name,
				&latest_3_releases[1].name,
				format!("{} ({})", &latest_3_releases[1].tag_name, &latest_3_releases[1].id),
			)
			.interact()?;
	} else {
		version = cliclack::select(format!("Select a template provider: "))
			.initial_value(&latest_3_releases[0].tag_name)
			.item(
				&latest_3_releases[0].tag_name,
				&latest_3_releases[0].name,
				format!("{} ({})", &latest_3_releases[0].tag_name, &latest_3_releases[0].id)
			)
			.interact()?;
	}
	Ok(version.to_string())
}

#[cfg(test)]
mod tests {

	use git2::Repository;

	use super::*;
	use std::fs;

	#[tokio::test]
	async fn test_new_parachain_command_execute() -> anyhow::Result<()> {
		let command = NewParachainCommand {
			name: Some("test_parachain".to_string()),
			provider: Some(Provider::Pop),
			template: Some(Template::Base),
			symbol: Some("UNIT".to_string()),
			decimals: Some(12),
			initial_endowment: Some("1u64 << 60".to_string()),
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
