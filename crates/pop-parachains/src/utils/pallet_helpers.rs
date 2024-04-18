use crate::errors::Error;
use std::{
	env::current_dir,
	fs,
	path::{Path, PathBuf},
	process,
};

/// Resolve pallet path
/// For a template it should be `<template>/pallets/`
/// For no path, it should just place it in the current working directory
pub fn resolve_pallet_path(path: Option<String>) -> Result<PathBuf, Error> {
	if let Some(path) = path {
		return Ok(Path::new(&path).to_path_buf());
	}

	let cwd = current_dir().map_err(|_| Error::CurrentDirAccess)?;

	let output = process::Command::new(env!("CARGO"))
		.arg("locate-project")
		.arg("--workspace")
		.arg("--message-format=plain")
		.output()
		.map_err(|_| Error::WorkspaceLocate)?
		.stdout;

	let workspace_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
	if workspace_path == Path::new("") {
		return Ok(cwd);
	}

	let pallet_path = workspace_path.parent().ok_or(Error::WorkspaceLocate)?.join("pallets");

	fs::create_dir_all(&pallet_path).map_err(|_| Error::PalletDirCreation)?;

	Ok(pallet_path)
}
