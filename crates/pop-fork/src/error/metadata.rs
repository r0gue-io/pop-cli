// SPDX-License-Identifier: GPL-3.0

//! ForkMetadata error types.

use crate::error::RpcClientError;
use thiserror::Error;

/// Errors that can occur when interacting with the metadata.
#[derive(Debug, Error)]
pub enum MetadataError {
	/// Failed to decode metadata
	#[error("Failed to decode metadata.")]
	DecodeError,
	/// RPC client error when fetching metadata
	#[error(transparent)]
	RpcError(#[from] RpcClientError),
}
