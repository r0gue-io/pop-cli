// SPDX-License-Identifier: GPL-3.0

use crate::{sourcing, templates};
use thiserror::Error;

/// Represents the various errors that can occur in the crate.
#[derive(Error, Debug)]
pub enum Error {
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	#[error("Configuration error: {0}")]
	Config(String),
	#[error("{0}")]
	Descriptive(String),
	#[error("a git error occurred: {0}")]
	Git(String),
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	/// An error occurred while attempting to create a keypair from the provided URI.
	#[error("Failed to create keypair from URI: {0}")]
	KeyPairCreation(String),
	#[error("Manifest error: {0}")]
	ManifestError(#[from] cargo_toml::Error),
	/// An error occurred while attempting to retrieve the manifest path.
	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	/// An error occurred while parsing the provided secret URI.
	#[error("Failed to parse secret URI: {0}")]
	ParseSecretURI(String),
	#[error("SourceError error: {0}")]
	SourceError(#[from] sourcing::Error),
	#[error("Syn parse error: {0}. Pop CLI has to parse your code in order to expand it. To preserve its structure while parsing, some temporal type markers may be added in the target part of your code. If declaring a type in that part of the code is invalid Rust code, that may be the origin of this error. Please review the code you're modifying to solve this. Example: If you're modifying an Enum likee the following one, it'll fail as types cannot be defined inside enums\npub enum Enum{{\n\t//This is the painful comment\n\tA,\n\tB\n}}")]
	SynError(#[from] syn::Error),
	#[error("TemplateError error: {0}")]
	TemplateError(#[from] templates::Error),
	#[error("TomlError: {0}")]
	TomlError(#[from] toml_edit::TomlError),
	#[error("Unsupported command: {0}")]
	UnsupportedCommand(String),
	#[error("Unsupported platform: {arch} {os}")]
	UnsupportedPlatform { arch: &'static str, os: &'static str },
	#[error("Unable to write to the introduced path. {0}")]
	WriteError(String),
}
