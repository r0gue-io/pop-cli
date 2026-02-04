// SPDX-License-Identifier: GPL-3.0

//! RPC server error types.
//!
//! This module defines error types for the RPC server, including
//! standard JSON-RPC error codes per the Substrate specification.

use jsonrpsee::types::ErrorObjectOwned;

/// Standard JSON-RPC error codes for Substrate RPC.
pub mod error_codes {
	/// Parse error - Invalid JSON was received.
	pub const PARSE_ERROR: i32 = -32700;

	/// Invalid request - The JSON sent is not a valid Request object.
	pub const INVALID_REQUEST: i32 = -32600;

	/// Method not found.
	pub const METHOD_NOT_FOUND: i32 = -32601;

	/// Invalid params.
	pub const INVALID_PARAMS: i32 = -32602;

	/// Internal error.
	pub const INTERNAL_ERROR: i32 = -32603;

	/// Too many follow subscriptions.
	pub const TOO_MANY_SUBSCRIPTIONS: i32 = -32800;

	/// Block hash not found or unpinned.
	pub const INVALID_BLOCK: i32 = -32801;

	/// Operation limit reached.
	pub const OPERATION_LIMIT: i32 = -32802;

	/// Operation not found.
	pub const OPERATION_NOT_FOUND: i32 = -32803;

	/// Invalid transaction - Transaction is invalid (code 1010).
	///
	/// This covers validation failures like:
	/// - Invalid signature
	/// - Nonce too low (stale)
	/// - Account cannot pay fees
	pub const INVALID_TRANSACTION: i32 = 1010;

	/// Unknown transaction - Transaction validity cannot be determined (code 1011).
	///
	/// This covers cases where the transaction might become valid later:
	/// - Nonce too high (future)
	/// - Dependencies not met
	pub const UNKNOWN_TRANSACTION: i32 = 1011;
}

/// Errors that can occur in the RPC server.
#[derive(Debug, thiserror::Error)]
pub enum RpcServerError {
	/// Failed to start the server.
	#[error("Failed to start RPC server: {0}")]
	ServerStart(String),

	/// Too many active subscriptions.
	#[error("Too many active subscriptions (limit: {limit})")]
	TooManySubscriptions {
		/// Maximum allowed subscriptions.
		limit: usize,
	},

	/// Block is not pinned or unknown.
	#[error("Block {hash} is not pinned or unknown")]
	BlockNotPinned {
		/// The block hash that was not found.
		hash: String,
	},

	/// Invalid subscription ID.
	#[error("Invalid subscription ID: {id}")]
	InvalidSubscription {
		/// The invalid subscription ID.
		id: String,
	},

	/// Operation not found.
	#[error("Operation {id} not found")]
	OperationNotFound {
		/// The operation ID that was not found.
		id: String,
	},

	/// Storage error.
	#[error("Storage error: {0}")]
	Storage(String),

	/// Runtime call failed.
	#[error("Runtime call failed: {0}")]
	RuntimeCall(String),

	/// Invalid parameter.
	#[error("Invalid parameter: {0}")]
	InvalidParam(String),

	/// Internal error.
	#[error("Internal error: {0}")]
	Internal(String),

	/// Block not found.
	#[error("Block not found: {0}")]
	BlockNotFound(String),

	/// Transaction validation failed - transaction is invalid.
	#[error("Transaction is invalid: {reason}")]
	InvalidTransaction {
		/// Human-readable reason for invalidity.
		reason: String,
		/// Raw error data from runtime (hex-encoded).
		data: Option<String>,
	},

	/// Transaction validity unknown - may become valid later.
	#[error("Transaction validity unknown: {reason}")]
	UnknownTransaction {
		/// Human-readable reason.
		reason: String,
		/// Raw error data from runtime (hex-encoded).
		data: Option<String>,
	},
}

impl From<RpcServerError> for ErrorObjectOwned {
	fn from(err: RpcServerError) -> Self {
		match err {
			RpcServerError::ServerStart(msg) =>
				ErrorObjectOwned::owned(error_codes::INTERNAL_ERROR, msg, None::<()>),
			RpcServerError::TooManySubscriptions { limit } => ErrorObjectOwned::owned(
				error_codes::TOO_MANY_SUBSCRIPTIONS,
				format!("Too many subscriptions (limit: {limit})"),
				None::<()>,
			),
			RpcServerError::BlockNotPinned { hash } => ErrorObjectOwned::owned(
				error_codes::INVALID_BLOCK,
				format!("Block {hash} is not pinned or unknown"),
				None::<()>,
			),
			RpcServerError::InvalidSubscription { id } => ErrorObjectOwned::owned(
				error_codes::INVALID_PARAMS,
				format!("Invalid subscription ID: {id}"),
				None::<()>,
			),
			RpcServerError::OperationNotFound { id } => ErrorObjectOwned::owned(
				error_codes::OPERATION_NOT_FOUND,
				format!("Operation {id} not found"),
				None::<()>,
			),
			RpcServerError::Storage(msg) =>
				ErrorObjectOwned::owned(error_codes::INTERNAL_ERROR, msg, None::<()>),
			RpcServerError::RuntimeCall(msg) =>
				ErrorObjectOwned::owned(error_codes::INTERNAL_ERROR, msg, None::<()>),
			RpcServerError::InvalidParam(msg) =>
				ErrorObjectOwned::owned(error_codes::INVALID_PARAMS, msg, None::<()>),
			RpcServerError::Internal(msg) =>
				ErrorObjectOwned::owned(error_codes::INTERNAL_ERROR, msg, None::<()>),
			RpcServerError::BlockNotFound(msg) =>
				ErrorObjectOwned::owned(error_codes::INVALID_BLOCK, msg, None::<()>),
			RpcServerError::InvalidTransaction { reason, data } => ErrorObjectOwned::owned(
				error_codes::INVALID_TRANSACTION,
				format!("Transaction is invalid: {reason}"),
				data,
			),
			RpcServerError::UnknownTransaction { reason, data } => ErrorObjectOwned::owned(
				error_codes::UNKNOWN_TRANSACTION,
				format!("Transaction validity unknown: {reason}"),
				data,
			),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn invalid_transaction_error_has_correct_code() {
		let error = RpcServerError::InvalidTransaction {
			reason: "Nonce too low".to_string(),
			data: Some("0x1234".to_string()),
		};
		let error_object: ErrorObjectOwned = error.into();

		assert_eq!(error_object.code(), error_codes::INVALID_TRANSACTION);
		assert_eq!(error_object.code(), 1010);
		assert!(error_object.message().contains("Transaction is invalid"));
		assert!(error_object.message().contains("Nonce too low"));
	}

	#[test]
	fn unknown_transaction_error_has_correct_code() {
		let error =
			RpcServerError::UnknownTransaction { reason: "Nonce too high".to_string(), data: None };
		let error_object: ErrorObjectOwned = error.into();

		assert_eq!(error_object.code(), error_codes::UNKNOWN_TRANSACTION);
		assert_eq!(error_object.code(), 1011);
		assert!(error_object.message().contains("Transaction validity unknown"));
		assert!(error_object.message().contains("Nonce too high"));
	}
}
