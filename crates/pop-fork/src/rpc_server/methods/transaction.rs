// SPDX-License-Identifier: GPL-3.0

//! Transaction RPC methods (v1 spec).
//!
//! These methods implement the new JSON-RPC spec for transaction submission.

use crate::{
	Blockchain, TxPool,
	rpc_server::{RpcServerError, parse_hex_bytes},
	strings::rpc_server::transaction,
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::sync::{
	Arc,
	atomic::{AtomicU64, Ordering},
};

/// Counter for generating unique operation IDs.
static OPERATION_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Generate a unique operation ID.
fn generate_operation_id() -> String {
	let id = OPERATION_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
	format!("{}-{id}", transaction::OPERATION_ID_PREFIX)
}

/// Transaction RPC methods (v1 spec).
#[rpc(server, namespace = "transaction")]
pub trait TransactionApi {
	/// Broadcast a transaction to the network.
	///
	/// Submits the transaction to the transaction pool and returns an operation ID.
	/// In instant mode, the transaction is immediately included in a block.
	#[method(name = "v1_broadcast")]
	async fn broadcast(&self, transaction: String) -> RpcResult<Option<String>>;

	/// Stop broadcasting a transaction.
	///
	/// This is a no-op in the fork since there is no P2P network.
	#[method(name = "v1_stop")]
	async fn stop(&self, operation_id: String) -> RpcResult<()>;
}

/// Implementation of transaction RPC methods.
pub struct TransactionApi {
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
		let tx_bytes = parse_hex_bytes(&transaction, "transaction")?;

		// Submit to TxPool
		self.txpool
			.submit(tx_bytes)
			.map_err(|e| RpcServerError::Internal(format!("Failed to submit transaction: {e}")))?;

		// Instant mode: immediately drain txpool and build block
		let pending_txs = self.txpool.drain().map_err(|e| {
			RpcServerError::Internal(format!("Failed to drain transaction pool: {e}"))
		})?;

		self.blockchain
			.build_block(pending_txs)
			.await
			.map_err(|e| RpcServerError::Internal(format!("Failed to build block: {e}")))?;

		// Return operation ID
		Ok(Some(generate_operation_id()))
	}

	async fn stop(&self, _operation_id: String) -> RpcResult<()> {
		// No-op: fork doesn't broadcast to P2P network
		Ok(())
	}
}
