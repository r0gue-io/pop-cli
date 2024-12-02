// SPDX-License-Identifier: GPL-3.0

use crate::{sourcing, templates};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	#[error("Configuration error: {0}")]
	Config(String),
	#[error("a git error occurred: {0}")]
	Git(String),
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	#[error("Failed to create keypair from URI: {0}")]
	KeyPairCreation(String),
	#[error("Manifest error: {0}")]
	ManifestError(#[from] cargo_toml::Error),
	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	#[error("Failed to parse secret URI: {0}")]
	ParseSecretURI(String),
	#[error("SourceError error: {0}")]
	SourceError(#[from] sourcing::Error),
	#[error("TemplateError error: {0}")]
	TemplateError(#[from] templates::Error),
	#[error("Unsupported command: {0}")]
	UnsupportedCommand(String),
	#[error("Unsupported platform: {arch} {os}")]
	UnsupportedPlatform { arch: &'static str, os: &'static str },
}
