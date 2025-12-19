// SPDX-License-Identifier: GPL-3.0

use serde::Serialize;

/// The standard response envelope for all CLI commands when run with --json.
#[derive(Debug, Serialize)]
pub struct CliResponse<T: Serialize> {
	/// The version of the schema. Always 1 for now.
	pub schema_version: u32,
	/// Whether the command was successful.
	pub success: bool,
	/// The data returned by the command on success.
	pub data: Option<T>,
	/// The error returned by the command on failure.
	pub error: Option<CliError>,
}

/// A structured error response.
#[derive(Debug, Serialize)]
pub struct CliError {
	/// A stable, machine-readable error code.
	pub code: ErrorCode,
	/// A short human-readable summary of the error.
	pub message: String,
	/// Optional extra context (stack trace, stderr snippet, etc.)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub details: Option<String>,
}

/// Machine-readable error codes.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
	/// Required user input was missing.
	PromptRequired,
	/// An invalid argument was provided.
	InvalidArgument,
	/// An I/O error occurred.
	IoError,
	/// A network-related error occurred.
	NetworkError,
	/// An RPC-related error occurred.
	RpcError,
	/// Transaction submission failed.
	TxSubmitFailed,
	/// Transaction execution failed.
	TxExecutionFailed,
	/// A subprocess failed to execute.
	SubprocessFailed,
	/// An internal error occurred.
	InternalError,
}

/// A generic success response data.
#[derive(Debug, Serialize)]
pub struct SuccessData {
	/// A human-readable message.
	pub message: String,
}

impl<T: Serialize> CliResponse<T> {
	/// Creates a successful response.
	pub fn success(data: T) -> Self {
		Self { schema_version: 1, success: true, data: Some(data), error: None }
	}

	/// Creates an error response.
	pub fn error(message: String, details: Option<String>) -> Self {
		Self {
			schema_version: 1,
			success: false,
			data: None,
			error: Some(CliError { code: ErrorCode::InternalError, message, details }),
		}
	}
}
