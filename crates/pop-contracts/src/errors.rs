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
}
