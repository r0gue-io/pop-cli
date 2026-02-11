// SPDX-License-Identifier: GPL-3.0

//! Legacy author_* RPC methods.
//!
//! These methods provide transaction submission for polkadot.js compatibility.
//! This implementation uses "Instant mode" where submitting an extrinsic
//! immediately builds a block containing it.

use crate::{
	Blockchain, FailedExtrinsic, TransactionValidityError, TxPool,
	rpc_server::{RpcServerError, parse_hex_bytes, types::HexString},
};
use jsonrpsee::{
	PendingSubscriptionSink,
	core::{RpcResult, SubscriptionResult},
	proc_macros::rpc,
};
use log::debug;
use std::sync::Arc;
use subxt::config::substrate::H256;

#[derive(Debug, Clone)]
pub struct AuthorBuildResult {
	pub block_number: u32,
	pub block_hash: H256,
	pub failed: Vec<FailedExtrinsic>,
}

#[async_trait::async_trait]
pub trait AuthorBlockchain: Send + Sync {
	async fn validate_extrinsic(&self, extrinsic: &[u8]) -> Result<(), TransactionValidityError>;
	async fn build_block(
		&self,
		extrinsics: Vec<Vec<u8>>,
	) -> Result<AuthorBuildResult, crate::BlockchainError>;
}

#[async_trait::async_trait]
impl AuthorBlockchain for Blockchain {
	async fn validate_extrinsic(&self, extrinsic: &[u8]) -> Result<(), TransactionValidityError> {
		Blockchain::validate_extrinsic(self, extrinsic).await.map(|_| ())
	}

	async fn build_block(
		&self,
		extrinsics: Vec<Vec<u8>>,
	) -> Result<AuthorBuildResult, crate::BlockchainError> {
		let result = Blockchain::build_block(self, extrinsics).await?;
		Ok(AuthorBuildResult {
			block_number: result.block.number,
			block_hash: result.block.hash,
			failed: result.failed,
		})
	}
}

/// Legacy author RPC methods.
#[rpc(server, namespace = "author")]
pub trait AuthorApi {
	/// Submit a fully formatted extrinsic for block inclusion.
	///
	/// In instant mode, this immediately builds a block containing the extrinsic.
	/// Returns the hash of the submitted extrinsic.
	#[method(name = "submitExtrinsic")]
	async fn submit_extrinsic(&self, extrinsic: String) -> RpcResult<String>;

	/// Submit an extrinsic and watch its status.
	///
	/// Returns a subscription that sends transaction lifecycle events:
	/// ready → broadcast → inBlock → finalized
	#[subscription(name = "submitAndWatchExtrinsic" => "extrinsicUpdate", unsubscribe = "unwatchExtrinsic", item = serde_json::Value)]
	async fn submit_and_watch_extrinsic(&self, extrinsic: String) -> SubscriptionResult;

	/// Get all pending extrinsics.
	///
	/// In instant mode, this usually returns an empty list since extrinsics
	/// are immediately included in blocks.
	#[method(name = "pendingExtrinsics")]
	async fn pending_extrinsics(&self) -> RpcResult<Vec<String>>;
}

/// Implementation of legacy author RPC methods.
pub struct AuthorApi<T: AuthorBlockchain = Blockchain> {
	blockchain: Arc<T>,
	txpool: Arc<TxPool>,
}

impl<T: AuthorBlockchain> AuthorApi<T> {
	/// Create a new AuthorApi instance.
	pub fn new(blockchain: Arc<T>, txpool: Arc<TxPool>) -> Self {
		Self { blockchain, txpool }
	}
}

#[async_trait::async_trait]
impl<T: AuthorBlockchain + 'static> AuthorApiServer for AuthorApi<T> {
	async fn submit_extrinsic(&self, extrinsic: String) -> RpcResult<String> {
		let ext_bytes = parse_hex_bytes(&extrinsic, "extrinsic")?;

		// Validate extrinsic before adding to pool.
		if let Err(err) = self.blockchain.validate_extrinsic(&ext_bytes).await {
			let reason = err.reason();
			let data = None; // Could encode the full error in future

			if err.is_unknown() {
				return Err(RpcServerError::UnknownTransaction { reason, data }.into());
			} else {
				return Err(RpcServerError::InvalidTransaction { reason, data }.into());
			}
		}

		// Instant mode: submit and immediately drain txpool in one operation.
		// This reduces lock acquisitions from 2 to 1.
		let (hash, pending_txs) = self
			.txpool
			.submit_and_drain(ext_bytes)
			.map_err(|e| RpcServerError::Internal(format!("Failed to submit extrinsic: {e}")))?;

		let result = self
			.blockchain
			.build_block(pending_txs)
			.await
			.map_err(|e| RpcServerError::Internal(format!("Failed to build block: {e}")))?;

		// Log any extrinsics that failed during dispatch (rare after validation)
		for failed in &result.failed {
			eprintln!("[AuthorApi] Extrinsic failed dispatch after validation: {}", failed.reason);
		}

		debug!(
			"[author] Extrinsic submitted (0x{}) included in block #{} (0x{})",
			hex::encode(hash.as_bytes()),
			result.block_number,
			hex::encode(&result.block_hash.as_bytes()[..4]),
		);

		Ok(HexString::from_bytes(hash.as_bytes()).into())
	}

	async fn submit_and_watch_extrinsic(
		&self,
		pending: PendingSubscriptionSink,
		extrinsic: String,
	) -> SubscriptionResult {
		let sink = pending.accept().await?;

		// Decode the hex extrinsic
		let ext_bytes = match hex::decode(extrinsic.trim_start_matches("0x")) {
			Ok(b) => b,
			Err(e) => {
				let msg = jsonrpsee::SubscriptionMessage::from_json(
					&serde_json::json!({"invalid": format!("Invalid hex: {e}")}),
				)?;
				let _ = sink.send(msg).await;
				return Ok(());
			},
		};

		// Validate before sending "ready" status.
		if let Err(err) = self.blockchain.validate_extrinsic(&ext_bytes).await {
			let msg = jsonrpsee::SubscriptionMessage::from_json(
				&serde_json::json!({"invalid": err.reason()}),
			)?;
			let _ = sink.send(msg).await;
			return Ok(());
		}

		// Calculate hash
		let hash = H256::from(sp_core::blake2_256(&ext_bytes));

		// Send "ready" status (only after validation passes)
		let msg = jsonrpsee::SubscriptionMessage::from_json(&serde_json::json!({"ready": null}))?;
		let _ = sink.send(msg).await;

		// Send "broadcast" status (simulated in fork - empty peer list)
		let msg = jsonrpsee::SubscriptionMessage::from_json(&serde_json::json!({"broadcast": []}))?;
		let _ = sink.send(msg).await;

		// Submit to TxPool
		if let Err(e) = self.txpool.submit(ext_bytes) {
			let msg = jsonrpsee::SubscriptionMessage::from_json(
				&serde_json::json!({"invalid": format!("Failed to submit: {e}")}),
			)?;
			let _ = sink.send(msg).await;
			return Ok(());
		}

		// Drain and build block (instant mode)
		let pending_txs = match self.txpool.drain() {
			Ok(txs) => txs,
			Err(e) => {
				let msg = jsonrpsee::SubscriptionMessage::from_json(
					&serde_json::json!({"dropped": format!("Pool error: {e}")}),
				)?;
				let _ = sink.send(msg).await;
				return Ok(());
			},
		};

		match self.blockchain.build_block(pending_txs).await {
			Ok(result) => {
				// Check if our extrinsic failed during dispatch
				// (in instant mode with single tx, if failed list is non-empty, our tx failed)
				if !result.failed.is_empty() {
					let msg = jsonrpsee::SubscriptionMessage::from_json(
						&serde_json::json!({"invalid": result.failed[0].reason}),
					)?;
					let _ = sink.send(msg).await;
					return Ok(());
				}

				let block_hex = format!("0x{}", hex::encode(result.block_hash.as_bytes()));

				debug!(
					"[author] Extrinsic submitted (0x{}) included in block #{} (0x{})",
					hex::encode(hash.as_bytes()),
					result.block_number,
					hex::encode(&result.block_hash.as_bytes()[..4]),
				);

				// Send "inBlock" status
				let msg = jsonrpsee::SubscriptionMessage::from_json(
					&serde_json::json!({"inBlock": block_hex}),
				)?;
				let _ = sink.send(msg).await;

				// Small delay then send "finalized" (fork has instant finality)
				tokio::time::sleep(std::time::Duration::from_millis(50)).await;

				let msg = jsonrpsee::SubscriptionMessage::from_json(
					&serde_json::json!({"finalized": block_hex}),
				)?;
				let _ = sink.send(msg).await;
			},
			Err(e) => {
				let msg = jsonrpsee::SubscriptionMessage::from_json(
					&serde_json::json!({"dropped": format!("Build failed: {e}")}),
				)?;
				let _ = sink.send(msg).await;
			},
		}

		Ok(())
	}

	async fn pending_extrinsics(&self) -> RpcResult<Vec<String>> {
		let pending = self.txpool.pending().map_err(|e| {
			RpcServerError::Internal(format!("Failed to get pending extrinsics: {e}"))
		})?;
		Ok(pending.iter().map(|ext| HexString::from_bytes(ext).into()).collect())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		InvalidTransaction, UnknownTransaction, rpc_server::test_scenarios::author as scenario,
	};
	use jsonrpsee::server::ServerBuilder;
	use std::sync::atomic::{AtomicUsize, Ordering};

	struct MockAuthorBlockchain {
		validate_error: Option<TransactionValidityError>,
		build_calls: AtomicUsize,
	}

	#[async_trait::async_trait]
	impl AuthorBlockchain for MockAuthorBlockchain {
		async fn validate_extrinsic(
			&self,
			_extrinsic: &[u8],
		) -> Result<(), TransactionValidityError> {
			match &self.validate_error {
				Some(err) => Err(err.clone()),
				None => Ok(()),
			}
		}

		async fn build_block(
			&self,
			_extrinsics: Vec<Vec<u8>>,
		) -> Result<AuthorBuildResult, crate::BlockchainError> {
			self.build_calls.fetch_add(1, Ordering::SeqCst);
			Ok(AuthorBuildResult {
				block_number: 11,
				block_hash: H256::from([0x11; 32]),
				failed: vec![],
			})
		}
	}

	fn mock_api(
		validate_error: Option<TransactionValidityError>,
	) -> (Arc<MockAuthorBlockchain>, AuthorApi<MockAuthorBlockchain>) {
		let blockchain =
			Arc::new(MockAuthorBlockchain { validate_error, build_calls: AtomicUsize::new(0) });
		let api = AuthorApi::new(blockchain.clone(), Arc::new(TxPool::new()));
		(blockchain, api)
	}

	async fn mock_ws_url(
		validate_error: Option<TransactionValidityError>,
	) -> (String, jsonrpsee::server::ServerHandle, Arc<MockAuthorBlockchain>) {
		let (mock, api) = mock_api(validate_error);
		let server =
			ServerBuilder::default().build("127.0.0.1:0").await.expect("server should bind");
		let addr = server.local_addr().expect("local addr should exist");
		let handle = server.start(AuthorApiServer::into_rpc(api));
		(format!("ws://{}", addr), handle, mock)
	}

	#[tokio::test]
	async fn author_submit_extrinsic_returns_correct_hash() {
		let extrinsic = vec![0xde, 0xad, 0xbe, 0xef];
		let expected_hash = format!("0x{}", hex::encode(sp_core::blake2_256(&extrinsic)));
		let ext_hex = format!("0x{}", hex::encode(&extrinsic));
		let (ws_url, handle, _) = mock_ws_url(None).await;
		scenario::author_submit_extrinsic_returns_correct_hash_at(
			&ws_url,
			&ext_hex,
			&expected_hash,
		)
		.await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn author_pending_extrinsics_empty_after_submit() {
		let (ws_url, handle, _) = mock_ws_url(None).await;
		scenario::author_pending_extrinsics_empty_after_submit_at(&ws_url, "0x0102").await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn author_submit_extrinsic_invalid_hex() {
		let (ws_url, handle, _) = mock_ws_url(None).await;
		scenario::author_submit_extrinsic_invalid_hex_at(&ws_url).await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn author_submit_extrinsic_rejects_garbage_with_error_code() {
		let (ws_url, handle, _) =
			mock_ws_url(Some(TransactionValidityError::Invalid(InvalidTransaction::BadSigner)))
				.await;
		scenario::author_submit_extrinsic_rejects_garbage_with_error_code_at(&ws_url, "0xdeadbeef")
			.await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn author_submit_extrinsic_does_not_build_block_on_validation_failure() {
		let (ws_url, handle, mock) =
			mock_ws_url(Some(TransactionValidityError::Unknown(UnknownTransaction::CannotLookup)))
				.await;
		scenario::author_submit_extrinsic_rejects_garbage_with_error_code_at(&ws_url, "0x0102")
			.await;
		assert_eq!(mock.build_calls.load(Ordering::SeqCst), 0);
		handle.stop().expect("server should stop");
	}
}
