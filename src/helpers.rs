use anyhow::Result;
use cliclack::{log, outro_cancel};
use git2::Repository;
use std::{
	env::current_dir,
	fs::{self, create_dir_all, OpenOptions},
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
pub(crate) fn resolve_pallet_path(path: Option<String>) -> PathBuf {
	if let Some(path) = path {
		return Path::new(&path).to_path_buf();
	}
	// Check if inside a project
	let cwd = current_dir().expect("current dir is inaccessible");
	match workspace_dir() {
		// No workspace available, return cwd.
		None => cwd,
		Some(dir) => {
			let pallet_target = dir.join("pallets");
			match create_dir_all(pallet_target.clone()) {
				Ok(_) => pallet_target,
				Err(_) => cwd, // Error creating folder.
			}
		},
	}
}

// Locate the workspace directory if any
pub(crate) fn workspace_dir() -> Option<PathBuf> {
	use std::process;

	let mut locate_project = process::Command::new(env!("CARGO"));
	let output = locate_project
		.arg("locate-project")
		.arg("--workspace")
		.arg("--message-format=plain")
		.output()
		.unwrap()
		.stdout;

	let maybe_workspace_path = Path::new(std::str::from_utf8(&output).unwrap().trim());

	if maybe_workspace_path == Path::new("") {
		None
	} else {
		Some(maybe_workspace_path.parent().unwrap().to_path_buf())
	}
}
