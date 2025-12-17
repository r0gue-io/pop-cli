// SPDX-License-Identifier: GPL-3.0

//! Cache-related error types.

use std::error::Error as StdError;
use thiserror::Error;

/// Errors that can occur when interacting with the storage cache.
#[derive(Debug, Error)]
pub enum CacheError {
	/// Database error.
	#[error("Database error: {0}")]
	Database(#[from] diesel::result::Error),
	/// Database connection error.
	#[error("Database connection error: {0}")]
	Connection(#[from] diesel::result::ConnectionError),
	/// Migration error.
	#[error("Migration error: {0}")]
	Migration(#[from] diesel_migrations::MigrationError),
	/// Connection pool get error (wrapping bb8 RunError).
	#[error("Connection pool get error: {0}")]
	PoolGet(#[from] diesel_async::pooled_connection::bb8::RunError),
	/// Connection pool build error.
	#[error("Connection pool build error: {0}")]
	PoolBuild(#[from] diesel_async::pooled_connection::PoolError),
	/// IO error.
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),
	/// Data corruption detected in the cache.
	#[error("Data corruption: {0}")]
	DataCorruption(String),
	/// Duplicated keys used
	#[error("Duplicated keys")]
	DuplicatedKeys,
}

impl From<Box<dyn StdError + Send + Sync>> for CacheError {
	fn from(e: Box<dyn StdError + Send + Sync>) -> Self {
		// Migrations return boxed errors; fold them into a descriptive error variant
		CacheError::DataCorruption(format!("{e}"))
	}
}
