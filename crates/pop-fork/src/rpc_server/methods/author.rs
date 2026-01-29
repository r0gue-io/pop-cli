// SPDX-License-Identifier: GPL-3.0

//! Legacy author_* RPC methods.
//!
//! These methods provide transaction submission for polkadot.js compatibility.
//! This implementation uses "Instant mode" where submitting an extrinsic
//! immediately builds a block containing it.

use crate::{Blockchain, TxPool};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
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
	/// NOTE: Subscriptions are not yet supported. Use `submitExtrinsic` instead.
	#[method(name = "submitAndWatchExtrinsic")]
	async fn submit_and_watch_extrinsic(&self, extrinsic: String) -> RpcResult<String>;

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
		// Decode the hex extrinsic
		let ext_bytes = hex::decode(extrinsic.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex extrinsic: {e}"),
				None::<()>,
			)
		})?;

		// Calculate hash before submitting
		let hash = H256::from(sp_core::blake2_256(&ext_bytes));

		// Submit to TxPool
		self.txpool.submit(ext_bytes).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to submit extrinsic: {e}"),
				None::<()>,
			)
		})?;

		// Instant mode: immediately drain txpool and build block.
		let pending_txs = self.txpool.drain().map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to drain transaction pool: {e}"),
				None::<()>,
			)
		})?;

		self.blockchain.build_block(pending_txs).await.map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to build block: {e}"),
				None::<()>,
			)
		})?;

		Ok(format!("0x{}", hex::encode(hash.as_bytes())))
	}

	async fn submit_and_watch_extrinsic(&self, _extrinsic: String) -> RpcResult<String> {
		// Subscriptions are not yet supported
		Err(jsonrpsee::types::ErrorObjectOwned::owned(
			-32601,
			"Method not supported: subscriptions are not yet implemented. Use author_submitExtrinsic instead.",
			None::<()>,
		))
	}

	async fn pending_extrinsics(&self) -> RpcResult<Vec<String>> {
		let pending = self.txpool.pending().map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to get pending extrinsics: {e}"),
				None::<()>,
			)
		})?;
		Ok(pending.iter().map(|ext| format!("0x{}", hex::encode(ext))).collect())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		ExecutorConfig, SignatureMockMode,
		rpc_server::{ForkRpcServer, RpcServerConfig},
	};
	use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
	use pop_common::test_env::TestNode;
	use scale::{Compact, Encode};
	use url::Url;

	/// Test context holding spawned node and RPC server.
	struct RpcTestContext {
		#[allow(dead_code)]
		node: TestNode,
		#[allow(dead_code)]
		server: ForkRpcServer,
		ws_url: String,
		blockchain: Arc<Blockchain>,
	}

	/// Well-known dev account: Alice
	const ALICE: [u8; 32] = [
		0xd4, 0x35, 0x93, 0xc7, 0x15, 0xfd, 0xd3, 0x1c, 0x61, 0x14, 0x1a, 0xbd, 0x04, 0xa9, 0x9f,
		0xd6, 0x82, 0x2c, 0x85, 0x58, 0x85, 0x4c, 0xcd, 0xe3, 0x9a, 0x56, 0x84, 0xe7, 0xa5, 0x6d,
		0xa2, 0x7d,
	];

	/// Well-known dev account: Bob
	const BOB: [u8; 32] = [
		0x8e, 0xaf, 0x04, 0x15, 0x16, 0x87, 0x73, 0x63, 0x26, 0xc9, 0xfe, 0xa1, 0x7e, 0x25, 0xfc,
		0x52, 0x87, 0x61, 0x36, 0x93, 0xc9, 0x12, 0x90, 0x9c, 0xb2, 0x26, 0xaa, 0x47, 0x94, 0xf2,
		0x6a, 0x48,
	];

	/// Transfer amount: 100 units (with 12 decimals)
	const TRANSFER_AMOUNT: u128 = 100_000_000_000_000;

	async fn setup_rpc_test() -> RpcTestContext {
		setup_rpc_test_with_config(ExecutorConfig::default()).await
	}

	/// Creates a test context with custom executor config.
	async fn setup_rpc_test_with_config(config: ExecutorConfig) -> RpcTestContext {
		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");

		let blockchain = Blockchain::fork_with_config(&endpoint, None, None, config)
			.await
			.expect("Failed to fork blockchain");
		let txpool = Arc::new(TxPool::new());

		let server = ForkRpcServer::start(blockchain.clone(), txpool, RpcServerConfig::default())
			.await
			.expect("Failed to start RPC server");

		let ws_url = server.ws_url();
		RpcTestContext { node, server, ws_url, blockchain }
	}

	/// Build a mock V4 signed extrinsic with dummy signature (from Alice).
	fn build_mock_signed_extrinsic_v4(call_data: &[u8]) -> Vec<u8> {
		let mut inner = Vec::new();
		inner.push(0x84); // Version: signed (0x80) + v4 (0x04)
		inner.push(0x00); // MultiAddress::Id variant
		inner.extend(ALICE);
		inner.extend([0u8; 64]); // Dummy signature (works with AlwaysValid)
		inner.push(0x00); // CheckMortality: immortal
		inner.extend(Compact(0u64).encode()); // CheckNonce
		inner.extend(Compact(0u128).encode()); // ChargeTransactionPayment
		inner.push(0x00); // EthSetOrigin: None
		inner.extend(call_data);
		let mut extrinsic = Compact(inner.len() as u32).encode();
		extrinsic.extend(inner);
		extrinsic
	}

	/// Compute Blake2_128Concat storage key for System::Account.
	fn account_storage_key(account: &[u8; 32]) -> Vec<u8> {
		let mut key = Vec::new();
		key.extend(sp_core::twox_128(b"System"));
		key.extend(sp_core::twox_128(b"Account"));
		key.extend(sp_core::blake2_128(account));
		key.extend(account);
		key
	}

	/// Decode AccountInfo and extract free balance.
	fn decode_free_balance(data: &[u8]) -> u128 {
		const ACCOUNT_DATA_OFFSET: usize = 16;
		u128::from_le_bytes(
			data[ACCOUNT_DATA_OFFSET..ACCOUNT_DATA_OFFSET + 16]
				.try_into()
				.expect("need 16 bytes for u128"),
		)
	}

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
		let ctx = setup_rpc_test_with_config(config).await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let initial_block_number = ctx.blockchain.head_number().await;

		// Get storage key for Alice
		let alice_key = account_storage_key(&ALICE);
		let alice_balance_before = ctx
			.blockchain
			.storage(&alice_key)
			.await
			.expect("Failed to get Alice balance")
			.map(|v| decode_free_balance(&v))
			.expect("Alice should have a balance");

		// Build a transfer extrinsic using metadata for correct pallet/call indices
		let call_data = build_transfer_call_data(&ctx.blockchain).await;

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
		let new_block_number = ctx.blockchain.head_number().await;

		let alice_balance_after = ctx
			.blockchain
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
		let ctx = setup_rpc_test_with_config(config).await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build and submit an extrinsic using metadata for correct pallet/call indices
		let call_data = build_transfer_call_data(&ctx.blockchain).await;

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
		let ctx = setup_rpc_test_with_config(config).await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a transfer extrinsic using metadata for correct pallet/call indices
		let call_data = build_transfer_call_data(&ctx.blockchain).await;

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
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Submit invalid hex
		let result: Result<String, _> =
			client.request("author_submitExtrinsic", rpc_params!["not_valid_hex"]).await;

		assert!(result.is_err(), "Should fail with invalid hex");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn author_submit_and_watch_returns_not_supported() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a valid extrinsic using metadata for correct pallet/call indices
		let call_data = build_transfer_call_data(&ctx.blockchain).await;

		let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
		let ext_hex = format!("0x{}", hex::encode(&extrinsic));

		// Try to call submitAndWatchExtrinsic - should fail with not supported
		let result: Result<String, _> =
			client.request("author_submitAndWatchExtrinsic", rpc_params![ext_hex]).await;

		assert!(result.is_err(), "Should fail with not supported error");
		let err = result.unwrap_err();
		assert!(
			err.to_string().contains("not supported") || err.to_string().contains("-32601"),
			"Error should indicate method not supported: {err}"
		);
	}
}
