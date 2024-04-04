use anyhow::Result;
use cliclack::{input, log, outro_cancel};
use git2::{IndexAddOption, Repository, ResetType};
use regex::Regex;
use std::{
	env::current_dir,
	fs::{self, OpenOptions},
	path::{Path, PathBuf},
};

use crate::{engines::templates::Config, git::TagInfo};

pub(crate) fn sanitize(target: &Path) -> Result<()> {
	use std::io::{stdin, stdout, Write};
	if target.exists() {
		print!("\"{}\" folder exists. Do you want to clean it? [y/n]: ", target.display());
		stdout().flush()?;

		let mut input = String::new();
		stdin().read_line(&mut input)?;

		if input.trim().to_lowercase() == "y" {
			fs::remove_dir_all(target)?;
		} else {
			return Err(anyhow::anyhow!("User aborted due to existing target folder."));
		}
	}
	Ok(())
}

pub(crate) fn write_to_file<'a>(path: &Path, contents: &'a str) {
	log::info(format!("Writing to {}", path.display())).ok();
	use std::io::Write;
	let mut file = OpenOptions::new().write(true).truncate(true).create(true).open(path).unwrap();
	file.write_all(contents.as_bytes()).unwrap();
	if path.extension().map_or(false, |ext| ext == "rs") {
		let output = std::process::Command::new("rustfmt")
			.arg(path.to_str().unwrap())
			.output()
			.expect("failed to execute rustfmt");

		if !output.status.success() {
			outro_cancel("rustfmt exited with non-zero status code.").ok();
		}
	}
}

/// Clone `url` into `target` and degit it
pub(crate) fn clone_and_degit(
	url: &str,
	target: &Path,
	tag_version: Option<String>,
) -> Result<Option<String>> {
	let repo = Repository::clone(url, target)?;
	// fetch tags from remote
	let release = fetch_latest_tag(&repo);

	if tag_version.is_some() {
		let tag = tag_version.clone().unwrap();
		let (object, reference) = repo.revparse_ext(&tag).expect("Object not found");
		repo.checkout_tree(&object, None).expect("Failed to checkout");
		match reference {
			// gref is an actual reference like branches or tags
			Some(gref) => repo.set_head(gref.name().unwrap()),
			// this is a commit, not a reference
			None => repo.set_head_detached(object.id()),
		}
		.expect("Failed to set HEAD");

		let git_dir = repo.path();
		fs::remove_dir_all(&git_dir)?;
		// Return release version selected by the user
		return Ok(tag_version);
	}
	let git_dir = repo.path();
	fs::remove_dir_all(&git_dir)?;
	// Or by default the last one
	Ok(release)
}

/// Fetch the latest release from a repository
fn fetch_latest_tag(repo: &Repository) -> Option<String> {
	let version_reg = Regex::new(r"v\d+\.\d+\.\d+").expect("Valid regex");
	let tags = repo.tag_names(None).ok()?;
	// Start from latest tags
	for tag in tags.iter().rev() {
		if let Some(tag) = tag {
			if version_reg.is_match(tag) {
				return Some(tag.to_string());
			}
		}
	}
	None
}

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

/// Resolve pallet path
/// For a template it should be `<template>/pallets/`
/// For no path, it should just place it in the current working directory
#[cfg(feature = "parachain")]
pub(crate) fn resolve_pallet_path(path: Option<String>) -> PathBuf {
	use std::process;

	if let Some(path) = path {
		return Path::new(&path).to_path_buf();
	}
	// Check if inside a template
	let cwd = current_dir().expect("current dir is inaccessible");

	let output = process::Command::new(env!("CARGO"))
		.arg("locate-project")
		.arg("--workspace")
		.arg("--message-format=plain")
		.output()
		.unwrap()
		.stdout;
	let workspace_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
	if workspace_path == Path::new("") {
		cwd
	} else {
		let pallet_path = workspace_path.parent().unwrap().to_path_buf().join("pallets");
		match fs::create_dir_all(pallet_path.clone()) {
			Ok(_) => pallet_path,
			Err(_) => cwd,
		}
	}
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_resolve_pallet_path_with_no_path() {
		let result = resolve_pallet_path(None);
		let working_path = std::env::current_dir().unwrap().join("pallets");
		assert_eq!(result, working_path);
	}

	#[test]
	fn test_resolve_pallet_path_with_custom_path() {
		let custom_path = tempfile::tempdir().expect("Failed to create temp dir");
		let custom_path_str = custom_path.path().join("my_pallets").to_str().unwrap().to_string();

		let result = resolve_pallet_path(Some(custom_path_str.clone()));

		assert_eq!(result, custom_path.path().join("my_pallets"), "Unexpected result path");
	}
}
