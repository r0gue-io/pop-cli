// SPDX-License-Identifier: GPL-3.0

//! Development RPC methods for manual chain control.
//!
//! These methods are not part of the Substrate RPC spec but are useful for
//! development and testing purposes.

use crate::{Blockchain, TxPool};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::sync::Arc;

/// Development RPC methods for manual chain control.
#[rpc(server, namespace = "dev")]
pub trait DevApi {
	/// Produce a new block manually.
	///
	/// This builds a new block on top of the current head, applying:
	/// 1. Inherent extrinsics (timestamp, parachain validation data, etc.)
	/// 2. Any pending transactions from the transaction pool
	///
	/// Returns the hash of the newly created block.
	#[method(name = "newBlock")]
	async fn new_block(&self) -> RpcResult<NewBlockResult>;
}

/// Result of producing a new block.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBlockResult {
	/// Hash of the new block.
	pub hash: String,
	/// Block number.
	pub number: u64,
	/// Number of extrinsics included (inherents + user transactions).
	pub extrinsics_count: usize,
}

/// Implementation of development RPC methods.
pub struct DevApi {
	blockchain: Arc<Blockchain>,
	txpool: Arc<TxPool>,
}

impl DevApi {
	/// Create a new DevApi instance.
	pub fn new(blockchain: Arc<Blockchain>, txpool: Arc<TxPool>) -> Self {
		Self { blockchain, txpool }
	}
}

#[async_trait::async_trait]
impl DevApiServer for DevApi {
	async fn new_block(&self) -> RpcResult<NewBlockResult> {
		// Drain pending transactions from the pool
		let pending_txs = self.txpool.drain().map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to drain transaction pool: {e}"),
				None::<()>,
			)
		})?;

		// Build a new block with the pending transactions
		let block = self.blockchain.build_block(pending_txs).await.map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to build block: {e}"),
				None::<()>,
			)
		})?;

		Ok(NewBlockResult {
			hash: format!("0x{}", hex::encode(block.hash.as_bytes())),
			number: block.number as u64,
			extrinsics_count: block.extrinsics.len(),
		})
	}
}
