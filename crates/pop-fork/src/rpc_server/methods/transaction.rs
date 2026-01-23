// SPDX-License-Identifier: GPL-3.0

//! New transaction_v1_* RPC methods.
//!
//! These methods follow the new Substrate JSON-RPC specification for transaction handling.

use crate::{Blockchain, TxPool};
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use std::sync::Arc;

/// New transaction RPC methods (v1 spec).
#[rpc(server, namespace = "transaction")]
pub trait TransactionApi {
	/// Broadcast a transaction.
	///
	/// Returns an operation ID if successful, or null if the transaction was rejected.
	#[method(name = "v1_broadcast")]
	async fn broadcast(&self, transaction: String) -> RpcResult<Option<String>>;

	/// Stop broadcasting a transaction.
	#[method(name = "v1_stop")]
	async fn stop(&self, operation_id: String) -> RpcResult<()>;
}

/// Implementation of transaction RPC methods.
pub struct TransactionApi {
	#[allow(dead_code)]
	blockchain: Arc<Blockchain>,
	txpool: Arc<TxPool>,
}

impl TransactionApi {
	/// Create a new TransactionApi instance.
	pub fn new(blockchain: Arc<Blockchain>, txpool: Arc<TxPool>) -> Self {
		Self { blockchain, txpool }
	}
}

#[async_trait::async_trait]
impl TransactionApiServer for TransactionApi {
	async fn broadcast(&self, transaction: String) -> RpcResult<Option<String>> {
		// Decode the hex transaction
		let tx_bytes = hex::decode(transaction.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex transaction: {e}"),
				None::<()>,
			)
		})?;

		// Submit to TxPool and return hash as operation ID
		let hash = self.txpool.submit(tx_bytes).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to broadcast transaction: {e}"),
				None::<()>,
			)
		})?;
		Ok(Some(format!("0x{}", hex::encode(hash.as_bytes()))))
	}

	async fn stop(&self, _operation_id: String) -> RpcResult<()> {
		// TxPool doesn't support cancellation, so this is a no-op
		Ok(())
	}
}
