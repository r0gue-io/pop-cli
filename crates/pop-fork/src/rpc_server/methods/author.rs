// SPDX-License-Identifier: GPL-3.0

//! Legacy author_* RPC methods.
//!
//! These methods provide transaction submission for polkadot.js compatibility.
//! This implementation uses "Instant mode" where submitting an extrinsic
//! immediately builds a block containing it.

use crate::{
	Blockchain, TxPool,
	rpc_server::{RpcServerError, parse_hex_bytes, types::HexString},
};
use jsonrpsee::{
	PendingSubscriptionSink,
	core::{RpcResult, SubscriptionResult},
	proc_macros::rpc,
};
use log::info;
use std::sync::Arc;
use subxt::config::substrate::H256;

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
pub struct AuthorApi {
	blockchain: Arc<Blockchain>,
	txpool: Arc<TxPool>,
}

impl AuthorApi {
	/// Create a new AuthorApi instance.
	pub fn new(blockchain: Arc<Blockchain>, txpool: Arc<TxPool>) -> Self {
		Self { blockchain, txpool }
	}
}

#[async_trait::async_trait]
impl AuthorApiServer for AuthorApi {
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

		info!(
			"[author] Extrinsic submitted (0x{}) included in block #{} (0x{})",
			hex::encode(hash.as_bytes()),
			result.block.number,
			hex::encode(&result.block.hash.as_bytes()[..4]),
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

				let block_hex = format!("0x{}", hex::encode(result.block.hash.as_bytes()));

				info!(
					"[author] Extrinsic submitted (0x{}) included in block #{} (0x{})",
					hex::encode(hash.as_bytes()),
					result.block.number,
					hex::encode(&result.block.hash.as_bytes()[..4]),
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
		ExecutorConfig, SignatureMockMode,
		testing::{
			TestContext, TestContextBuilder,
			accounts::{ALICE, BOB},
			constants::TRANSFER_AMOUNT,
			helpers::{account_storage_key, build_mock_signed_extrinsic_v4, decode_free_balance},
		},
	};
	use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
	use scale::{Compact, Encode};

	/// Build call data for Balances.transfer_keep_alive using metadata.
	async fn build_transfer_call_data(blockchain: &Blockchain) -> Vec<u8> {
		let head = blockchain.head().await;
		let metadata = head.metadata().await.expect("Failed to get metadata");

		let balances_pallet =
			metadata.pallet_by_name("Balances").expect("Balances pallet should exist");
		let pallet_index = balances_pallet.index();
		let transfer_call = balances_pallet
			.call_variant_by_name("transfer_keep_alive")
			.expect("transfer_keep_alive call should exist");
		let call_index = transfer_call.index;

		let mut call_data = vec![pallet_index, call_index];
		call_data.push(0x00); // MultiAddress::Id variant
		call_data.extend(BOB);
		call_data.extend(Compact(TRANSFER_AMOUNT).encode());
		call_data
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_submit_extrinsic_builds_block_immediately() {
		let config =
			ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
		let ctx = TestContext::for_rpc_server_with_config(config).await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		let initial_block_number = ctx.blockchain().head_number().await;

		// Get storage key for Alice
		let alice_key = account_storage_key(&ALICE);
		let alice_balance_before = ctx
			.blockchain()
			.storage(&alice_key)
			.await
			.expect("Failed to get Alice balance")
			.map(|v| decode_free_balance(&v))
			.expect("Alice should have a balance");

		// Build a transfer extrinsic using metadata for correct pallet/call indices
		let call_data = build_transfer_call_data(ctx.blockchain()).await;

		let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
		let ext_hex = format!("0x{}", hex::encode(&extrinsic));

		// Submit the extrinsic
		let hash: String = client
			.request("author_submitExtrinsic", rpc_params![ext_hex])
			.await
			.expect("RPC call failed");

		// Hash should be returned
		assert!(hash.starts_with("0x"), "Hash should start with 0x");
		assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");

		// Block should have been built immediately
		let new_block_number = ctx.blockchain().head_number().await;

		let alice_balance_after = ctx
			.blockchain()
			.storage(&alice_key)
			.await
			.expect("Failed to get Alice balance after")
			.map(|v| decode_free_balance(&v))
			.expect("Alice should still have a balance");
		assert_eq!(
			new_block_number,
			initial_block_number + 1,
			"Block should have been built immediately"
		);

		assert_ne!(alice_balance_after, alice_balance_before);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_pending_extrinsics_empty_after_submit() {
		let config =
			ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
		let ctx = TestContext::for_rpc_server_with_config(config).await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Build and submit an extrinsic using metadata for correct pallet/call indices
		let call_data = build_transfer_call_data(ctx.blockchain()).await;

		let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
		let ext_hex = format!("0x{}", hex::encode(&extrinsic));

		// Submit the extrinsic
		let _hash: String = client
			.request("author_submitExtrinsic", rpc_params![ext_hex])
			.await
			.expect("RPC call failed");

		// Check pending extrinsics - should be empty in instant mode
		let pending: Vec<String> = client
			.request("author_pendingExtrinsics", rpc_params![])
			.await
			.expect("RPC call failed");

		assert!(
			pending.is_empty(),
			"Pending extrinsics should be empty after submit in instant mode"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_submit_extrinsic_returns_correct_hash() {
		let config =
			ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
		let ctx = TestContext::for_rpc_server_with_config(config).await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Build a transfer extrinsic using metadata for correct pallet/call indices
		let call_data = build_transfer_call_data(ctx.blockchain()).await;

		let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
		let expected_hash = H256::from(sp_core::blake2_256(&extrinsic));
		let ext_hex = format!("0x{}", hex::encode(&extrinsic));

		// Submit the extrinsic
		let hash: String = client
			.request("author_submitExtrinsic", rpc_params![ext_hex])
			.await
			.expect("RPC call failed");

		// Verify the hash matches
		assert_eq!(hash, format!("0x{}", hex::encode(expected_hash.as_bytes())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_submit_extrinsic_invalid_hex() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Submit invalid hex
		let result: Result<String, _> =
			client.request("author_submitExtrinsic", rpc_params!["not_valid_hex"]).await;

		assert!(result.is_err(), "Should fail with invalid hex");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_submit_and_watch_sends_lifecycle_events() {
		use jsonrpsee::core::client::SubscriptionClientT;

		let config =
			ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
		let ctx = TestContext::for_rpc_server_with_config(config).await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		let initial_block_number = ctx.blockchain().head_number().await;

		// Build a valid extrinsic using metadata for correct pallet/call indices
		let call_data = build_transfer_call_data(ctx.blockchain()).await;
		let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
		let ext_hex = format!("0x{}", hex::encode(&extrinsic));

		// Subscribe to extrinsic status
		let mut subscription: jsonrpsee::core::client::Subscription<serde_json::Value> = client
			.subscribe(
				"author_submitAndWatchExtrinsic",
				rpc_params![ext_hex],
				"author_unwatchExtrinsic",
			)
			.await
			.expect("Failed to subscribe");

		// Collect events with timeout
		let mut events = Vec::new();
		let timeout = tokio::time::Duration::from_secs(10);

		loop {
			match tokio::time::timeout(timeout, subscription.next()).await {
				Ok(Some(Ok(event))) => {
					let is_finalized = event.get("finalized").is_some();
					events.push(event);
					if is_finalized {
						break;
					}
				},
				Ok(Some(Err(e))) => panic!("Subscription error: {e}"),
				Ok(None) => break, // Stream ended
				Err(_) => panic!("Timeout waiting for subscription events"),
			}
		}

		// Verify we got the expected lifecycle events
		assert!(
			events.len() >= 3,
			"Should receive at least ready, inBlock, finalized events. Got: {:?}",
			events
		);

		// First event should be "ready"
		assert!(
			events[0].get("ready").is_some(),
			"First event should be 'ready', got: {:?}",
			events[0]
		);

		// Should have "broadcast"
		assert!(
			events.iter().any(|e| e.get("broadcast").is_some()),
			"Should have 'broadcast' event"
		);

		// Should have "inBlock"
		let in_block_event = events.iter().find(|e| e.get("inBlock").is_some());
		assert!(in_block_event.is_some(), "Should have 'inBlock' event");

		// Should have "finalized"
		let finalized_event = events.iter().find(|e| e.get("finalized").is_some());
		assert!(finalized_event.is_some(), "Should have 'finalized' event");

		// Block should have been built
		let new_block_number = ctx.blockchain().head_number().await;
		assert_eq!(new_block_number, initial_block_number + 1, "Block should have been built");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_submit_extrinsic_rejects_garbage_with_error_code() {
		let ctx = TestContextBuilder::new().with_server().build().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Submit garbage bytes
		let garbage_hex = "0xdeadbeef";

		let result: Result<String, _> =
			client.request("author_submitExtrinsic", rpc_params![garbage_hex]).await;

		assert!(result.is_err(), "Garbage should be rejected");

		// Verify we get an error (the specific code depends on how the runtime rejects it)
		let err = result.unwrap_err();
		let err_str = err.to_string();
		// Error should indicate invalid transaction
		assert!(
			err_str.contains("1010") ||
				err_str.contains("1011") ||
				err_str.contains("invalid") ||
				err_str.contains("Invalid"),
			"Error should indicate transaction invalidity: {err_str}"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_submit_extrinsic_does_not_build_block_on_validation_failure() {
		let ctx = TestContextBuilder::new().with_server().build().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		let initial_block_number = ctx.blockchain().head_number().await;

		// Submit garbage bytes
		let garbage_hex = "0xdeadbeef";

		let _result: Result<String, _> =
			client.request("author_submitExtrinsic", rpc_params![garbage_hex]).await;

		// Block number should NOT have changed (no block built for invalid tx)
		let new_block_number = ctx.blockchain().head_number().await;
		assert_eq!(
			initial_block_number, new_block_number,
			"Block should NOT be built when extrinsic validation fails"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_submit_and_watch_sends_invalid_on_validation_failure() {
		use jsonrpsee::core::client::SubscriptionClientT;

		let ctx = TestContextBuilder::new().with_server().build().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Submit garbage bytes via subscription
		let garbage_hex = "0xdeadbeef";

		let mut subscription: jsonrpsee::core::client::Subscription<serde_json::Value> = client
			.subscribe(
				"author_submitAndWatchExtrinsic",
				rpc_params![garbage_hex],
				"author_unwatchExtrinsic",
			)
			.await
			.expect("Failed to subscribe");

		// Should receive "invalid" event (not "ready")
		let timeout = tokio::time::Duration::from_secs(5);
		match tokio::time::timeout(timeout, subscription.next()).await {
			Ok(Some(Ok(event))) => {
				assert!(
					event.get("invalid").is_some(),
					"First event should be 'invalid' for garbage input, got: {:?}",
					event
				);
			},
			Ok(Some(Err(e))) => panic!("Subscription error: {e}"),
			Ok(None) => panic!("Subscription ended without events"),
			Err(_) => panic!("Timeout waiting for subscription event"),
		}
	}
}
