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
	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),
	#[error("Manifest error: {0}")]
	ManifestError(#[from] cargo_toml::Error),
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	#[error("SourceError error: {0}")]
	SourceError(#[from] sourcing::Error),
	#[error("Syn parse error: {0}. To preserve your not-docs comments, blank lines and declarative macro invocations, Pop-CLi temporarily transform them to comments followed by a marker type associated to that doc. This error is likely originated cause one of your files has such an element in a place where that marker type cannot be placed. Example: the type marker cannot be defined inside a match block\nmatch option{{\n\t//This is the painful comment\n\tSome(some)=>(),\n\tNone=>()\n}}")]
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
