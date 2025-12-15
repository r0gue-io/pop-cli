// SPDX-License-Identifier: GPL-3.0

use crate::{sourcing, templates};
use thiserror::Error;

/// Represents the various errors that can occur in the crate.
#[derive(Error, Debug)]
pub enum Error {
	/// An error occurred while parsing the provided account address.
	#[error("Failed to parse account address: {0}")]
	AccountAddressParsing(String),
	/// An error occurred.
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	/// A configuration error occurred.
	#[error("Configuration error: {0}")]
	Config(String),
	/// An error regarding Docker happened.
	#[error("Docker error: {0}")]
	Docker(String),
	/// A Git error occurred.
	#[error("a git error occurred: {0}")]
	Git(String),
	/// An IO error occurred.
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	/// An error occurred while attempting to create a keypair from the provided URI.
	#[error("Failed to create keypair from URI: {0}")]
	KeyPairCreation(String),
	/// A manifest error occurred.
	#[error("Manifest error: {0}")]
	ManifestError(#[from] cargo_toml::Error),
	/// An error occurred while attempting to retrieve the manifest path.
	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),
	/// An error occurred during parsing.
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	/// An error occurred while parsing the provided secret URI.
	#[error("Failed to parse secret URI: {0}")]
	ParseSecretURI(String),
	/// An error occurred during sourcing of a binary.
	#[error("SourceError error: {0}")]
	SourceError(#[from] sourcing::Error),
	/// A template error occurred.
	#[error("TemplateError error: {0}")]
	TemplateError(#[from] templates::Error),
	/// An error occurred while executing a test command.
	#[error("Failed to execute test command: {0}")]
	TestCommand(String),
	/// The command is unsupported.
	#[error("Unsupported command: {0}")]
	UnsupportedCommand(String),
	/// The platform is unsupported.
	#[error("Unsupported platform: {arch} {os}")]
	UnsupportedPlatform {
		/// The architecture of the CPU that is currently in use.
		arch: &'static str,
		/// The operating system in use.
		os: &'static str,
	},
}
