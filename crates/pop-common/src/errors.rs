// SPDX-License-Identifier: GPL-3.0

use crate::templates;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Configuration error: {0}")]
	Config(String),
	#[error("a git error occurred: {0}")]
	Git(String),
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	#[error("TemplateError error: {0}")]
	TemplateError(#[from] templates::Error),
	#[error("Toml error: {0}")]
	TomlError(#[from] toml_edit::de::Error),
}
