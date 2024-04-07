use anyhow::Result;
use cliclack::{confirm, input, log, outro_cancel};
use git2::{IndexAddOption, Repository, ResetType};
use std::{fs, path::Path};

use crate::{
	engines::templates::{Config, Provider, Template},
	git::TagInfo,
};
/// Init a new git repo on creation of a parachain
pub(crate) fn git_init(target: &Path, message: &str) -> Result<(), git2::Error> {
	let repo = Repository::init(target)?;
	let signature = repo.signature()?;

	let mut index = repo.index()?;
	index.add_all(["*"].iter(), IndexAddOption::DEFAULT, None)?;
	let tree_id = index.write_tree()?;

	let tree = repo.find_tree(tree_id)?;
	let commit_id = repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])?;

	let commit_object = repo.find_object(commit_id, Some(git2::ObjectType::Commit))?;
	repo.reset(&commit_object, ResetType::Hard, None)?;

	Ok(())
}

pub(crate) fn get_customization_value(
	template: &Template,
	symbol: Option<String>,
	decimals: Option<u8>,
	initial_endowment: Option<String>,
) -> Result<Config> {
	if !matches!(template, Template::Base)
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

pub(crate) fn is_template_supported(provider: &Provider, template: &Template) -> Result<()> {
	if !template.is_provider_correct(provider) {
		return Err(anyhow::anyhow!(format!(
			"The provider \"{}\" doesn't support the {} template.",
			provider, template
		)));
	};
	return Ok(());
}

pub fn display_release_versions_to_user(releases: Vec<TagInfo>) -> Result<String> {
	let mut prompt = cliclack::select("Select a specific release:".to_string());
	for (i, release) in releases.iter().enumerate() {
		if i == 0 {
			prompt = prompt.initial_value(&release.tag_name);
		}
		prompt = prompt.item(
			&release.tag_name,
			&release.name,
			format!("{} / {}", &release.tag_name, &release.commit[..=6]),
		)
	}
	Ok(prompt.interact()?.to_string())
}

pub fn prompt_customizable_options() -> Result<Config> {
	let symbol: String = input("What is the symbol of your parachain token?")
		.placeholder("UNIT")
		.default_input("UNIT")
		.interact()?;

	let decimals_input: String = input("How many token decimals?")
		.placeholder("12")
		.default_input("12")
		.interact()?;
	let decimals: u8 = decimals_input.parse::<u8>().expect("input has to be a number");

	let endowment: String = input("And the initial endowment for dev accounts?")
		.placeholder("1u64 << 60")
		.default_input("1u64 << 60")
		.interact()?;
	Ok(Config { symbol, decimals, initial_endowment: endowment })
}

pub fn check_destination_path(name_template: &String) -> Result<&Path> {
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_get_customization_values() {
		let config = get_customization_value(
			&Template::Base,
			Some("DOT".to_string()),
			Some(6),
			Some("10000".to_string()),
		);
		assert!(config.is_ok());
		assert_eq!(
			config.unwrap(),
			Config {
				symbol: "DOT".to_string(),
				decimals: 6,
				initial_endowment: "10000".to_string()
			}
		);
	}
	#[test]
	fn test_is_template_supported() {
		assert!(is_template_supported(&Provider::Pop, &Template::Base).is_ok());
		assert!(is_template_supported(&Provider::Pop, &Template::OZTemplate).is_err());
		assert!(is_template_supported(&Provider::Pop, &Template::ParityContracts).is_err());
		assert!(is_template_supported(&Provider::Pop, &Template::ParityFPT).is_err());

		assert!(is_template_supported(&Provider::OpenZeppelin, &Template::Base).is_err());
		assert!(is_template_supported(&Provider::OpenZeppelin, &Template::OZTemplate).is_ok());
		assert!(is_template_supported(&Provider::OpenZeppelin, &Template::ParityContracts).is_err());
		assert!(is_template_supported(&Provider::OpenZeppelin, &Template::ParityFPT).is_err());

		assert!(is_template_supported(&Provider::Parity, &Template::Base).is_err());
		assert!(is_template_supported(&Provider::Parity, &Template::OZTemplate).is_err());
		assert!(is_template_supported(&Provider::Parity, &Template::ParityContracts).is_ok());
		assert!(is_template_supported(&Provider::Parity, &Template::ParityFPT).is_ok());
	}
}
