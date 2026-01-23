// SPDX-License-Identifier: GPL-3.0

//! New transaction_v1_* RPC methods.
//!
//! These methods follow the new Substrate JSON-RPC specification for transaction handling.

use crate::rpc_server::MockBlockchain;
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
	blockchain: Arc<MockBlockchain>,
}

impl TransactionApi {
	/// Create a new TransactionApi instance.
	pub fn new(blockchain: Arc<MockBlockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl TransactionApiServer for TransactionApi {
	async fn broadcast(&self, transaction: String) -> RpcResult<Option<String>> {
		// Decode the hex transaction
		let _tx_bytes = hex::decode(transaction.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex transaction: {e}"),
				None::<()>,
			)
		})?;

		// Mock: Would submit to TxPool and return operation ID
		// For now, return a mock operation ID
		Ok(Some("mock-broadcast-1".to_string()))
	}

	async fn stop(&self, _operation_id: String) -> RpcResult<()> {
		// Mock: no-op
		Ok(())
	}
}
