// SPDX-License-Identifier: GPL-3.0

//! String constants for the cache module.

/// SQLite PRAGMA statements for connection configuration.
pub mod pragmas {
	/// Sets SQLite busy timeout to 5 seconds to reduce lock errors under contention.
	pub const BUSY_TIMEOUT: &str = "PRAGMA busy_timeout=5000;";
	/// Enables Write-Ahead Logging for better concurrency on file databases.
	pub const JOURNAL_MODE_WAL: &str = "PRAGMA journal_mode=WAL;";
}

/// Database connection URLs.
pub mod urls {
	/// SQLite in-memory database URL.
	pub const IN_MEMORY: &str = ":memory:";
}

/// Error message strings for data validation.
pub mod errors {
	/// Message for block number outside valid u32 range.
	pub const BLOCK_NUMBER_OUT_OF_U32_RANGE: &str = "block number out of u32 range";
}

/// Patterns used to detect SQLite lock-related errors.
pub mod lock_patterns {
	/// SQLite "database is locked" error message pattern.
	pub const DATABASE_IS_LOCKED: &str = "database is locked";
	/// SQLite "busy" error message pattern.
	pub const BUSY: &str = "busy";
}
