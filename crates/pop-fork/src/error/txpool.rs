// SPDX-License-Identifier: GPL-3.0

//! Txpool error types.

use thiserror::Error;

/// Errors that can occur when accessing the transaction pool.
#[derive(Debug, Error)]
pub enum TxPoolError {
	/// Failed to acquire lock on the transaction pool.
	#[error("TxPool acquire error: {0}")]
	Lock(String),
}
