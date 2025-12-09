// SPDX-License-Identifier: GPL-3.0

//! Cache-related error types.

use thiserror::Error;

/// Errors that can occur when interacting with the storage cache.
#[derive(Debug, Error)]
pub enum CacheError {
	/// Database error.
	#[error("Database error: {0}")]
	Database(#[from] sqlx::Error),
	/// IO error.
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),
	/// Data corruption detected in the cache.
	#[error("Data corruption: {0}")]
	DataCorruption(String),
}
