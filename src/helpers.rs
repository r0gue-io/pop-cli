#![allow(unused)]
use anyhow::Result;
use cliclack::{log, outro_cancel};
use git2::Repository;
use std::{
	env::current_dir,
	fs::{self, OpenOptions},
	path::{Path, PathBuf},
};

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
pub(crate) fn clone_and_degit(url: &str, target: &Path) -> Result<()> {
	let repo = Repository::clone(url, target)?;
	let git_dir = repo.path();
	fs::remove_dir_all(&git_dir)?;
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

pub(crate) fn is_git_repo_with_commits(repo_path: &Path) -> anyhow::Result<bool> {
	match Repository::open(repo_path) {
		Ok(repo) => {
			let mut status_opts = git2::StatusOptions::new();
			status_opts.include_untracked(false);
			let statuses = repo.statuses(Some(&mut status_opts))?;

			// Check if there are no changes to commit
			Ok(if !statuses.iter().any(|s| s.status() != git2::Status::CURRENT) {
				true
			} else {
				log::warning(format!("Repository has uncommitted changes."));
				false
			})
		},
		Err(e) => {
			log::error(format!("Failed to open repository: {}", e));
			Err(e.into())
		},
	}
}
