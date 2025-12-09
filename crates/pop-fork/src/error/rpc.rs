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
	#[error("RPC request failed: {0}")]
	RequestFailed(String),
	/// Invalid response from RPC.
	#[error("Invalid RPC response: {0}")]
	InvalidResponse(String),
	/// Storage key not found (this is different from empty storage).
	#[error("Required storage key not found: {0}")]
	StorageNotFound(String),
}
