// SPDX-License-Identifier: GPL-3.0

//! Txpool error types.

use thiserror::Error;

/// Errors that can occur when accessing the local storage layer.
#[derive(Debug, Error)]
pub enum TxPoolError {
	#[error("TxPool acquire error: {0}")]
	Lock(String),
}
