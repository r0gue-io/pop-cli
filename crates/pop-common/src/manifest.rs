// SPDX-License-Identifier: GPL-3.0

pub use cargo_toml::Manifest;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// An error specific to reading a manifest.
#[derive(Error, Debug)]
pub enum Error {
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
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
	Ok(Manifest::from_path(path.canonicalize()?)?)
}

#[cfg(test)]
mod tests {
	use crate::manifest::from_path;
	use anyhow::{Error, Result};
	use std::path::Path;

	#[test]
	fn from_path_works() -> Result<(), Error> {
		// Workspace manifest from directory
		from_path(Some(Path::new("../../")))?;
		// Workspace manifest from path
		from_path(Some(Path::new("../../Cargo.toml")))?;
		// Package manifest from directory
		from_path(Some(Path::new(".")))?;
		// Package manifest from path
		from_path(Some(Path::new("./Cargo.toml")))?;
		// None
		from_path(None)?;
		Ok(())
	}

	#[test]
	fn from_path_ensures_manifest_exists() -> Result<(), Error> {
		assert!(matches!(
			from_path(Some(Path::new("./none.toml"))),
			Err(super::Error::ManifestPath(..))
		));
		Ok(())
	}
}
