// SPDX-License-Identifier: GPL-3.0

//! Local storage layer error types.

use crate::error::{CacheError, RpcClientError};
use thiserror::Error;

/// Errors that can occur when accessing the local storage layer.
#[derive(Debug, Error)]
pub enum LocalStorageError {
	/// RPC client error when fetching from the live chain.
	#[error("RPC error: {0}")]
	Rpc(#[from] RpcClientError),
	/// Cache error when storing/retrieving cached values.
	#[error("Cache error: {0}")]
	Cache(#[from] CacheError),
    /// Lock acquire error
    #[error("Local storage acquire error: {0}")]
    Lock(String)
}
