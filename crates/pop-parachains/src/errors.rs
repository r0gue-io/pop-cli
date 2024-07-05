// SPDX-License-Identifier: GPL-3.0

use thiserror::Error;
use zombienet_sdk::OrchestratorError;

#[derive(Error, Debug)]
pub enum Error {
	#[error("User aborted due to existing target folder.")]
	Aborted,
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	#[error("Archive error: {0}")]
	ArchiveError(String),
	#[error("{0}")]
	CommonError(#[from] pop_common::Error),
	#[error("Configuration error: {0}")]
	Config(String),
	#[error("Failed to access the current directory")]
	CurrentDirAccess,
	#[error("Failed to parse the endowment value")]
	EndowmentError,
	#[error("HTTP error: {0}")]
	HttpError(#[from] reqwest::Error),
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	#[error("Missing binary: {0}")]
	MissingBinary(String),
	#[error("Missing chain spec file at: {0}")]
	MissingChainSpec(String),
	#[error("Command {command} doesn't exist in binary {binary}")]
	MissingCommand { command: String, binary: String },
	#[error("Orchestrator error: {0}")]
	OrchestratorError(#[from] OrchestratorError),
	#[error("Failed to create pallet directory")]
	PalletDirCreation,
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	#[error("Failed to execute rustfmt")]
	RustfmtError(std::io::Error),
	#[error("Template error: {0}")]
	TemplateError(#[from] pop_common::templates::Error),
	#[error("Toml error: {0}")]
	TomlError(#[from] toml_edit::de::Error),
	#[error("Unsupported command: {0}")]
	UnsupportedCommand(String),
	#[error("Failed to locate the workspace")]
	WorkspaceLocate,
	#[error("Unsupported platform: {arch} {os}")]
	UnsupportedPlatform { arch: &'static str, os: &'static str },
}
