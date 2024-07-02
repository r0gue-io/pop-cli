// SPDX-License-Identifier: GPL-3.0

use crate::templates;
use thiserror::Error;
use zombienet_sdk::OrchestratorError;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Failed to access the current directory")]
	CurrentDirAccess,

	#[error("Failed to locate the workspace")]
	WorkspaceLocate,

	#[error("Failed to create pallet directory")]
	PalletDirCreation,

	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),

	#[error("HTTP error: {0}")]
	HttpError(#[from] reqwest::Error),

	#[error("Missing binary: {0}")]
	MissingBinary(String),

	#[error("Missing chain spec file at: {0}")]
	MissingChainSpec(String),

	#[error("Command {command} doesn't exist in binary {binary}")]
	MissingCommand { command: String, binary: String },

	#[error("Configuration error: {0}")]
	Config(String),

	#[error("Unsupported command: {0}")]
	UnsupportedCommand(String),

	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),

	#[error("Orchestrator error: {0}")]
	OrchestratorError(#[from] OrchestratorError),

	#[error("Toml error: {0}")]
	TomlError(#[from] toml_edit::de::Error),

	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),

	#[error("User aborted due to existing target folder.")]
	Aborted,

	#[error("Failed to execute rustfmt")]
	RustfmtError(std::io::Error),

	#[error("Template error: {0}")]
	TemplateError(#[from] templates::Error),

	#[error("Failed to parse the endowment value")]
	EndowmentError,

	#[error("Unsupported platform: {arch} {os}")]
	UnsupportedPlatform { arch: &'static str, os: &'static str },

	#[error("Archive error: {0}")]
	ArchiveError(String),
}
