use std::{
	env::current_dir,
	fs,
	path::{Path, PathBuf},
	process,
};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResolvePalletError {
	#[error("Failed to access the current directory")]
	CurrentDirAccessFailed,
	#[error("Failed to locate the workspace")]
	WorkspaceLocateFailed,
	#[error("Failed to create pallet directory")]
	PalletDirCreationFailed,
}

/// Resolve pallet path
/// For a template it should be `<template>/pallets/`
/// For no path, it should just place it in the current working directory
pub fn resolve_pallet_path(path: Option<String>) -> Result<PathBuf, ResolvePalletError> {
	if let Some(path) = path {
		return Ok(Path::new(&path).to_path_buf());
	}

	let cwd = current_dir().map_err(|_| ResolvePalletError::CurrentDirAccessFailed)?;

	let output = process::Command::new(env!("CARGO"))
		.arg("locate-project")
		.arg("--workspace")
		.arg("--message-format=plain")
		.output()
		.map_err(|_| ResolvePalletError::WorkspaceLocateFailed)?
		.stdout;

	let workspace_path = Path::new(std::str::from_utf8(&output).unwrap().trim());
	if workspace_path == Path::new("") {
		return Ok(cwd);
	}

	let pallet_path = workspace_path
		.parent()
		.ok_or(ResolvePalletError::WorkspaceLocateFailed)?
		.join("pallets");

	fs::create_dir_all(&pallet_path).map_err(|_| ResolvePalletError::PalletDirCreationFailed)?;

	Ok(pallet_path)
}
