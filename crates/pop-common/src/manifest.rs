// SPDX-License-Identifier: GPL-3.0

pub use cargo_toml::Manifest;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// An error specific to reading a manifest.
#[derive(Error, Debug)]
pub enum Error {
	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),
	#[error("Manifest error: {0}")]
	ManifestError(#[from] cargo_toml::Error),
}

/// Parses the contents of a `Cargo.toml` manifest.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not specified.
pub fn from_path(path: Option<&Path>) -> Result<Manifest, Error> {
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
	Ok(Manifest::from_path(path)?)
}
