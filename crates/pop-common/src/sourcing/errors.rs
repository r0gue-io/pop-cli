// SPDX-License-Identifier: GPL-3.0

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Anyhow error: {0}")]
	AnyhowError(#[from] anyhow::Error),
	#[error("Archive error: {0}")]
	ArchiveError(String),
	#[error("HTTP error: {0}")]
	HttpError(#[from] reqwest::Error),
	#[error("IO error: {0}")]
	IO(#[from] std::io::Error),
	#[error("Missing binary: {0}")]
	MissingBinary(String),
	#[error("ParseError error: {0}")]
	ParseError(#[from] url::ParseError),
	#[error("Unsupported command: {0}")]
	UnsupportedCommand(String),
	#[error("Unsupported platform: {arch} {os}")]
	UnsupportedPlatform { arch: &'static str, os: &'static str },
}
