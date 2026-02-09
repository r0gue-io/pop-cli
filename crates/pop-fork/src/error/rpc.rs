// SPDX-License-Identifier: GPL-3.0

//! RPC client error types.

use thiserror::Error;

/// Errors that can occur when interacting with the RPC client.
#[derive(Debug, Error)]
pub enum RpcClientError {
	/// Failed to connect to the RPC endpoint.
	#[error("Failed to connect to {endpoint}: {message}")]
	ConnectionFailed {
		/// The endpoint URL that failed to connect.
		endpoint: String,
		/// The error message describing the failure.
		message: String,
	},
	/// RPC request failed.
	#[error("RPC request `{method}` failed: {message}")]
	RequestFailed {
		/// The RPC method that failed.
		method: &'static str,
		/// The error message describing the failure.
		message: String,
	},
	/// RPC request timed out.
	#[error("RPC request `{method}` timed out")]
	Timeout {
		/// The RPC method that timed out.
		method: &'static str,
	},
	/// Invalid response from RPC.
	#[error("Invalid RPC response: {0}")]
	InvalidResponse(String),
	/// Storage key not found (this is different from empty storage).
	#[error("Required storage key not found: {0}")]
	StorageNotFound(String),
	/// Failed to decode metadata from remote chain.
	#[error("Metadata decoding failed: {0}")]
	MetadataDecodingFailed(String),
}
