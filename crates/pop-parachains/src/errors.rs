// SPDX-License-Identifier: GPL-3.0

use thiserror::Error;
use zombienet_sdk::OrchestratorError;

#[derive(Error, Debug)]
pub enum Error {
	#[error("User aborted due to existing target directory.")]
	Aborted,
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	#[error("Missing chain {0}")]
	ChainNotFound(String),
	#[error("{0}")]
	CommonError(#[from] pop_common::Error),
	#[error("Configuration error: {0}")]
	Config(String),
	#[error("Failed to access the current directory")]
	CurrentDirAccess,
	#[error("Failed to parse the endowment value")]
	EndowmentError,
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	#[error("JSON error: {0}")]
	JsonError(#[from] serde_json::Error),
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
	#[error("Invalid path")]
	PathError,
	#[error("Failed to execute rustfmt")]
	RustfmtError(std::io::Error),
	#[error("Template error: {0}")]
	SourcingError(#[from] pop_common::sourcing::Error),
	#[error("Toml error: {0}")]
	TomlError(#[from] toml_edit::de::Error),
	#[error("Unsupported command: {0}")]
	UnsupportedCommand(String),
	#[error("Failed to locate the workspace")]
	WorkspaceLocate,
}
