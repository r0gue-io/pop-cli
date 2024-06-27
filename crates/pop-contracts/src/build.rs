// SPDX-License-Identifier: GPL-3.0

use crate::utils::helpers::get_manifest_path;
use contract_build::{execute, BuildMode, ExecuteArgs};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),
	#[error("Manifest error: {0}")]
	ManifestError(#[from] cargo_toml::Error),
}

/// Build the smart contract located at the specified `path` in `build_release` mode.
pub fn build_smart_contract(path: Option<&Path>, build_release: bool) -> anyhow::Result<String> {
	let manifest_path = get_manifest_path(path)?;

	let build_mode = match build_release {
		true => BuildMode::Release,
		false => BuildMode::Debug,
	};
	// Default values
	let args = ExecuteArgs { manifest_path, build_mode, ..Default::default() };

	// Execute the build and log the output of the build
	let result = execute(args)?;
	let formatted_result = result.display();

	Ok(formatted_result)
}

/// Determines whether the manifest at the supplied path is a supported smart contract project.
pub fn is_supported(path: Option<&Path>) -> Result<bool, Error> {
	// Resolve manifest path
	let path = match path {
		Some(path) => match path.ends_with("Cargo.toml") {
			true => path.to_path_buf(),
			false => path.join("Cargo.toml"),
		},
		None => PathBuf::from("./Cargo.toml"),
	};
	if !path.exists() {
		return Err(Error::ManifestPath(path.display().to_string()));
	}
	let manifest = cargo_toml::Manifest::from_path(path)?;
	// Simply check for the `ink` dependency
	Ok(manifest.dependencies.contains_key("ink"))
}
