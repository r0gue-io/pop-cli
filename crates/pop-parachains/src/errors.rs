// SPDX-License-Identifier: GPL-3.0

use thiserror::Error;
use zombienet_sdk::OrchestratorError;

#[derive(Error, Debug)]
pub enum Error {
	#[error("User aborted due to existing target directory.")]
	Aborted,
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	#[error("Failed to establish a connection to the API: {0}")]
	ApiConnectionFailure(String),
	#[error("Failed to decode call data. {0}")]
	CallDataDecodingError(String),
	#[error("Failed to encode call data. {0}")]
	CallDataEncodingError(String),
	#[error("{0}")]
	CommonError(#[from] pop_common::Error),
	#[error("Configuration error: {0}")]
	Config(String),
	#[error("Failed to access the current directory")]
	CurrentDirAccess,
	#[error("Failed to parse the endowment value")]
	EndowmentError,
	#[error("The extrinsic is not supported")]
	ExtrinsicNotSupported,
	#[error("Extrinsic submission error: {0}")]
	ExtrinsicSubmissionError(String),
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	#[error("JSON error: {0}")]
	JsonError(#[from] serde_json::Error),
	#[error("Error parsing metadata for parameter {0} conversion")]
	MetadataParsingError(String),
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
	#[error("Failed to find the pallet {0}")]
	PalletNotFound(String),
	#[error("Failed to process the arguments provided by the user.")]
	ParamProcessingError,
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
