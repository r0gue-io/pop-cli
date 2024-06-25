// SPDX-License-Identifier: GPL-3.0
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Failed to create new contract project: {0}")]
	NewContract(String),

	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),

	#[error("Failed to execute test command: {0}")]
	TestCommand(String),

	#[error("Failed to parse balance: {0}")]
	BalanceParsing(String),

	#[error("Failed to parse account address: {0}")]
	AccountAddressParsing(String),

	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),

	#[error("Failed to parse secret URI: {0}")]
	ParseSecretURI(String),

	#[error("Failed to create keypair from URI: {0}")]
	KeyPairCreation(String),

	#[error("Failed to parse hex encoded bytes: {0}")]
	HexParsing(String),

	#[error("Pre-submission dry-run failed: {0}")]
	DryRunUploadContractError(String),

	#[error("Pre-submission dry-run failed: {0}")]
	DryRunCallContractError(String),

	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),

	#[error("Failed to install {0}")]
	InstallContractsNode(String),

	#[error("Failed to run {0}")]
	UpContractsNode(String),

	#[error("Unsupported platform: {os}")]
	UnsupportedPlatform { os: &'static str },

	#[error("HTTP error: {0}")]
	HttpError(#[from] reqwest::Error),

	#[error("a git error occurred: {0}")]
	Git(String),
}
