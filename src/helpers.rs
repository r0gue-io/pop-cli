use anyhow::Result;
use cliclack::{log, outro_cancel};
use git2::{IndexAddOption, Repository, ResetType};
use std::{
	env::current_dir,
	fs::{self, OpenOptions},
	path::{Path, PathBuf},
};

use crate::git::TagInfo;

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
pub(crate) fn clone_and_degit(url: &str, target: &Path, tag_version: Option<String>) -> Result<()> {
	let repo = Repository::clone(url, target)?;
	if tag_version.is_some() {
		let tag = tag_version.unwrap();
		let (object, reference) = repo.revparse_ext(&tag).expect("Object not found");
		repo.checkout_tree(&object, None).expect("Failed to checkout");
		match reference {
			// gref is an actual reference like branches or tags
			Some(gref) => repo.set_head(gref.name().unwrap()),
			// this is a commit, not a reference
			None => repo.set_head_detached(object.id()),
		}
		.expect("Failed to set HEAD");
	}
	let git_dir = repo.path();
	fs::remove_dir_all(&git_dir)?;
	Ok(())
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

pub fn display_release_versions_to_user(latest_3_releases: Vec<TagInfo>) -> Result<String> {
	let version;
	if latest_3_releases.len() == 3 {
		version = cliclack::select(format!("Select a specific release:"))
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
			.item(
				&latest_3_releases[2].tag_name,
				&latest_3_releases[2].name,
				format!("{} ({})", &latest_3_releases[2].tag_name, &latest_3_releases[2].id),
			)
			.interact()?;
	} else if latest_3_releases.len() == 2 {
		version = cliclack::select(format!("Select a specific release:"))
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
		version = cliclack::select(format!("Select a specific release:"))
			.initial_value(&latest_3_releases[0].tag_name)
			.item(
				&latest_3_releases[0].tag_name,
				&latest_3_releases[0].name,
				format!("{} ({})", &latest_3_releases[0].tag_name, &latest_3_releases[0].id),
			)
			.interact()?;
	}
	Ok(version.to_string())
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
