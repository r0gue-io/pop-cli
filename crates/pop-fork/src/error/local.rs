// SPDX-License-Identifier: GPL-3.0

//! Local storage layer error types.

use crate::error::{CacheError, RemoteStorageError, RpcClientError};
use thiserror::Error;

/// Errors that can occur when accessing the local storage layer.
#[derive(Debug, Error)]
pub enum LocalStorageError {
	/// Arithmetic error
	#[error("Arithmetic error")]
	Arithmetic,
	/// Cache error
	#[error(transparent)]
	Cache(#[from] CacheError),
	/// Remote storage error
	#[error(transparent)]
	RemoteStorage(#[from] RemoteStorageError),
	/// RPC client error when fetching metadata from remote
	#[error("RPC error: {0}")]
	Rpc(#[from] RpcClientError),
	/// Lock acquire error
	#[error("Local storage acquire error: {0}")]
	Lock(String),
	/// Metadata not found for the requested block
	#[error("Metadata not found: {0}")]
	MetadataNotFound(String),
}
