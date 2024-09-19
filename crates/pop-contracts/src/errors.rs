// SPDX-License-Identifier: GPL-3.0

use pop_common::sourcing::Error as SourcingError;
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	#[error("Failed to parse account address: {0}")]
	AccountAddressParsing(String),
	#[error("Failed to parse balance: {0}")]
	BalanceParsing(String),
	#[error("{0}")]
	CallContractError(String),
	#[error("{0}")]
	CommonError(#[from] pop_common::Error),
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
	#[error("{0}")]
	InstantiateContractError(String),
	#[error("Invalid name: {0}")]
	InvalidName(String),
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	#[error("Failed to create keypair from URI: {0}")]
	KeyPairCreation(String),
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
	#[error("{0}")]
	UploadContractError(String),
	#[error("Sourcing error {0}")]
	SourcingError(SourcingError),
}
