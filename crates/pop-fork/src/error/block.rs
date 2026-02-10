// SPDX-License-Identifier: GPL-3.0

use crate::error::{LocalStorageError, RemoteStorageError, RpcClientError};
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

	/// Remote storage layer error.
	#[error("Remote storage error: {0}")]
	RemoteStorage(#[from] RemoteStorageError),

	/// Block not found at the specified hash.
	#[error("Block not found: {0:?}")]
	BlockHashNotFound(H256),

	/// Block not found at the specified height.
	#[error("Block not found: {0:?}")]
	BlockNumberNotFound(u32),

	/// Runtime code not found in storage.
	#[error("Runtime code not found in storage")]
	RuntimeCodeNotFound,

	/// Concurrent block build detected - parent block changed during build.
	#[error("Concurrent block build detected: parent block changed during building")]
	ConcurrentBlockBuild,
}
