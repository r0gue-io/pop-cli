// SPDX-License-Identifier: GPL-3.0

use crate::error::{LocalStorageError, RpcClientError};
use subxt::config::substrate::H256;
use thiserror::Error;

/// Errors that can occur when working with blocks.
#[derive(Debug, Error)]
pub enum BlockError {
	/// RPC error while fetching block data.
	#[error("RPC error: {0}")]
	Rpc(#[from] RpcClientError),

	/// Storage layer error.
	#[error("Storage error: {0}")]
	Storage(#[from] LocalStorageError),

	/// Block not found at the specified hash.
	#[error("Block not found: {0:?}")]
	BlockHashNotFound(H256),

	/// Block not found at the specified height.
	#[error("Block not found: {0:?}")]
	BlockNumberNotFound(u32),
}
