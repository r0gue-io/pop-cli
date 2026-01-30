// SPDX-License-Identifier: GPL-3.0

use pop_common::sourcing::Error as SourcingError;
use thiserror::Error;

/// Represents the various errors that can occur in the crate.
#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum Error {
	/// An error occurred.
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	/// Failed to parse a balance value from a string representation.
	#[error("Failed to parse balance: {0}")]
	BalanceParsing(String),
	/// Failed to call the smart contract.
	#[error("{0}")]
	CallContractError(String),
	/// A contract metadata error happened
	#[error("{0}")]
	ContractMetadata(String),
	/// A common error originating from `pop_common`.
	#[error("{0}")]
	CommonError(#[from] pop_common::Error),
	/// Dry-run contract upload failed.
	#[error("Pre-submission dry-run failed: {0}")]
	DryRunUploadContractError(String),
	/// Dry-run contract call failed.
	#[error("Pre-submission dry-run failed: {0}")]
	DryRunCallContractError(String),
	/// Failed to parse hex-encoded bytes.
	#[error("Failed to parse hex encoded bytes: {0}")]
	HexParsing(String),
	/// An HTTP request failed.
	#[error("HTTP error: {0}")]
	HttpError(#[from] reqwest::Error),
	/// The number of provided arguments did not match the expected count.
	#[error("Incorrect number of arguments provided. Expecting {expected}, {provided} provided")]
	IncorrectArguments {
		/// Number of arguments expected.
		expected: usize,
		/// Number of arguments provided.
		provided: usize,
	},
	/// Failed to install the contracts node binary.
	#[error("Failed to install {0}")]
	InstallContractsNode(String),
	/// Contract instantiation failed.
	#[error("{0}")]
	InstantiateContractError(String),
	/// The specified constructor name is invalid.
	#[error("Invalid constructor name: {0}")]
	InvalidConstructorName(String),
	/// The specified message name is invalid.
	#[error("Invalid message name: {0}")]
	InvalidMessageName(String),
	/// The specified name is invalid.
	#[error("Invalid name: {0}")]
	InvalidName(String),
	/// The current toolchain is invalid
	#[error("Invalid toolchain: {0}")]
	InvalidToolchain(String),
	/// An I/O error occurred.
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	/// An I/O error occurred.
	#[error("Failed to get manifest path: {0}")]
	ManifestPath(String),
	/// Error returned when mapping an account fails.
	#[error("Failed to map account: {0}")]
	MapAccountError(String),
	/// A required argument was not provided.
	#[error("Argument {0} is required")]
	MissingArgument(String),
	/// An error occurred while creating a new contract project.
	#[error("Failed to create new contract project: {0}")]
	NewContract(String),
	/// An error occurred while parsing a URL.
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	/// The `Repository` property is missing from the template variant.
	#[error("The `Repository` property is missing from the template variant")]
	RepositoryMissing,
	/// A `[serde_json::Error]` error occurred
	#[error("Serde JSON error: {0}")]
	SerdeJson(#[from] serde_json::Error),
	/// An error occurred sourcing a binary.
	#[error("Sourcing error {0}")]
	SourcingError(SourcingError),
	/// An error occurred while executing a test command.
	#[error("Failed to execute test command: {0}")]
	TestCommand(String),
	/// The platform is unsupported.
	#[error("Unsupported platform: {os}")]
	UnsupportedPlatform {
		/// The operating system in use.
		os: &'static str,
	},
	/// An error occurred while uploading the contract.
	#[error("{0}")]
	UploadContractError(String),
	/// An error occurred while verifying a contract
	#[error("{0}")]
	Verification(String),
}
