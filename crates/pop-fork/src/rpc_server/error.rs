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

	/// Too many follow subscriptions (limit: 2 per connection).
	pub const TOO_MANY_SUBSCRIPTIONS: i32 = -32800;

	/// Block hash not found or unpinned.
	pub const INVALID_BLOCK: i32 = -32801;

	/// Operation limit reached.
	pub const OPERATION_LIMIT: i32 = -32802;

	/// Operation not found.
	pub const OPERATION_NOT_FOUND: i32 = -32803;
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
}

impl From<RpcServerError> for ErrorObjectOwned {
	fn from(err: RpcServerError) -> Self {
		match err {
			RpcServerError::ServerStart(msg) => {
				ErrorObjectOwned::owned(error_codes::INTERNAL_ERROR, msg, None::<()>)
			},
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
			RpcServerError::Storage(msg) => {
				ErrorObjectOwned::owned(error_codes::INTERNAL_ERROR, msg, None::<()>)
			},
			RpcServerError::RuntimeCall(msg) => {
				ErrorObjectOwned::owned(error_codes::INTERNAL_ERROR, msg, None::<()>)
			},
			RpcServerError::InvalidParam(msg) => {
				ErrorObjectOwned::owned(error_codes::INVALID_PARAMS, msg, None::<()>)
			},
			RpcServerError::Internal(msg) => {
				ErrorObjectOwned::owned(error_codes::INTERNAL_ERROR, msg, None::<()>)
			},
			RpcServerError::BlockNotFound(msg) => {
				ErrorObjectOwned::owned(error_codes::INVALID_BLOCK, msg, None::<()>)
			},
		}
	}
}
