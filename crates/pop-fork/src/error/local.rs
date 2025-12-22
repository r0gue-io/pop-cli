// SPDX-License-Identifier: GPL-3.0

//! Local storage layer error types.

use crate::error::RemoteStorageError;
use crate::error::CacheError;
use thiserror::Error;

/// Errors that can occur when accessing the local storage layer.
#[derive(Debug, Error)]
pub enum LocalStorageError {
    /// Cache error
    #[error(transparent)]
    Cache(#[from] CacheError),
	/// Remote storage error
	#[error(transparent)]
	RemoteStorage(#[from] RemoteStorageError),
	/// Lock acquire error
	#[error("Local storage acquire error: {0}")]
	Lock(String),
}
