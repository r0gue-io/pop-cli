// SPDX-License-Identifier: GPL-3.0

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	#[error("Failed to parse account address: {0}")]
	AccountAddressParsing(String),
	#[error("Failed to parse balance: {0}")]
	BalanceParsing(String),
	#[error("Pre-submission dry-run failed: {0}")]
	DryRunUploadContractError(String),
	#[error("Pre-submission dry-run failed: {0}")]
	DryRunCallContractError(String),
	#[error("Failed to parse hex encoded bytes: {0}")]
	HexParsing(String),
	#[error("HTTP error: {0}")]
	HttpError(#[from] reqwest::Error),
	#[error("Failed to install {0}")]
	InstallContractsNode(String),
	#[error("Invalid name: {0}")]
	InvalidName(String),
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	#[error("Failed to create keypair from URI: {0}")]
	KeyPairCreation(String),
	#[error("Manifest error: {0}")]
	ManifestError(#[from] pop_common::manifest::Error),
	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),
	#[error("Failed to create new contract project: {0}")]
	NewContract(String),
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	#[error("Failed to parse secret URI: {0}")]
	ParseSecretURI(String),
	#[error("The `Repository` property is missing from the template variant")]
	RepositoryMissing,
	#[error("Failed to execute test command: {0}")]
	TestCommand(String),
	#[error("Unsupported platform: {os}")]
	UnsupportedPlatform { os: &'static str },
}
