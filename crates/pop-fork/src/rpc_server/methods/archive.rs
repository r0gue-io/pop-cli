// SPDX-License-Identifier: GPL-3.0

//! New archive_v1_* RPC methods.
//!
//! These methods follow the new Substrate JSON-RPC specification for archive nodes.

use crate::{
	Blockchain,
	rpc_server::types::{
		ArchiveCallResult, ArchiveStorageItem, ArchiveStorageResult, StorageQueryItem,
	},
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::sync::Arc;
use subxt::config::substrate::H256;

/// New archive RPC methods (v1 spec).
#[rpc(server, namespace = "archive")]
pub trait ArchiveApi {
	/// Get the current finalized block height.
	#[method(name = "v1_finalizedHeight")]
	async fn finalized_height(&self) -> RpcResult<u32>;

	/// Get block hash by height.
	///
	/// Returns an array of hashes (returns an Option<Vec> to comply with the spec but, in practice,
	/// this Vec always contains a single element, as blocks are produced on-demand one by one).
	#[method(name = "v1_hashByHeight")]
	async fn hash_by_height(&self, height: u32) -> RpcResult<Option<Vec<String>>>;

	/// Get block header by hash.
	///
	/// Returns hex-encoded SCALE-encoded header.
	#[method(name = "v1_header")]
	async fn header(&self, hash: String) -> RpcResult<Option<String>>;

	/// Get block body by hash.
	///
	/// Returns array of hex-encoded extrinsics.
	#[method(name = "v1_body")]
	async fn body(&self, hash: String) -> RpcResult<Option<Vec<String>>>;

	/// Execute a runtime call at a block.
	///
	/// Returns `null` if the block is not found.
	#[method(name = "v1_call")]
	async fn call(
		&self,
		hash: String,
		function: String,
		call_parameters: String,
	) -> RpcResult<Option<ArchiveCallResult>>;

	/// Query storage at a finalized block.
	#[method(name = "v1_storage")]
	async fn storage(
		&self,
		hash: String,
		items: Vec<StorageQueryItem>,
		child_trie: Option<String>,
	) -> RpcResult<ArchiveStorageResult>;

	/// Stop a storage query operation.
	#[method(name = "v1_stopStorage")]
	async fn stop_storage(&self, operation_id: String) -> RpcResult<()>;

	/// Get the genesis hash.
	#[method(name = "v1_genesisHash")]
	async fn genesis_hash(&self) -> RpcResult<String>;
}

/// Implementation of archive RPC methods.
pub struct ArchiveApi {
	blockchain: Arc<Blockchain>,
}

impl ArchiveApi {
	/// Create a new ArchiveApi instance.
	pub fn new(blockchain: Arc<Blockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl ArchiveApiServer for ArchiveApi {
	async fn finalized_height(&self) -> RpcResult<u32> {
		Ok(self.blockchain.head_number().await)
	}

	async fn hash_by_height(&self, height: u32) -> RpcResult<Option<Vec<String>>> {
		// Fetch block hash (checks local blocks first, then remote)
		match self.blockchain.block_hash_at(height).await {
			Ok(Some(hash)) => Ok(Some(vec![format!("0x{}", hex::encode(hash.as_bytes()))])),
			Ok(None) => Ok(None),
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to fetch block hash: {e}"),
				None::<()>,
			)),
		}
	}

	async fn header(&self, hash: String) -> RpcResult<Option<String>> {
		// Parse the hash
		let hash_bytes = hex::decode(hash.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex hash: {e}"),
				None::<()>,
			)
		})?;

		let head = self.blockchain.head().await;
		let head_hash_bytes = head.hash.as_bytes();

		// Only return header if it matches the current head
		if hash_bytes == head_hash_bytes {
			Ok(Some(format!("0x{}", hex::encode(&head.header))))
		} else {
			Ok(None)
		}
	}

	async fn body(&self, hash: String) -> RpcResult<Option<Vec<String>>> {
		// Parse the hash
		let hash_bytes = hex::decode(hash.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex hash: {e}"),
				None::<()>,
			)
		})?;

		// Convert to H256
		let block_hash = H256::from_slice(&hash_bytes);

		// Fetch block body (checks local blocks first, then remote)
		match self.blockchain.block_body(block_hash).await {
			Ok(Some(extrinsics)) => {
				let hex_extrinsics: Vec<String> =
					extrinsics.iter().map(|ext| format!("0x{}", hex::encode(ext))).collect();
				Ok(Some(hex_extrinsics))
			},
			Ok(None) => Ok(None),
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to fetch block body: {e}"),
				None::<()>,
			)),
		}
	}

	async fn call(
		&self,
		hash: String,
		function: String,
		call_parameters: String,
	) -> RpcResult<Option<ArchiveCallResult>> {
		// Parse the hash
		let hash_bytes = hex::decode(hash.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex hash: {e}"),
				None::<()>,
			)
		})?;
		let block_hash = H256::from_slice(&hash_bytes);

		// Decode parameters
		let params = hex::decode(call_parameters.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex parameters: {e}"),
				None::<()>,
			)
		})?;

		// Execute the call at the specified block
		match self.blockchain.call_at_block(block_hash, &function, &params).await {
			Ok(Some(result)) =>
				Ok(Some(ArchiveCallResult::ok(format!("0x{}", hex::encode(result))))),
			Ok(None) => Ok(None), // Block not found
			Err(e) => Ok(Some(ArchiveCallResult::err(e.to_string()))),
		}
	}

	async fn storage(
		&self,
		_hash: String,
		items: Vec<StorageQueryItem>,
		_child_trie: Option<String>,
	) -> RpcResult<ArchiveStorageResult> {
		// Query storage for each item
		let mut results = Vec::new();
		for item in items {
			let key_bytes = hex::decode(item.key.trim_start_matches("0x")).map_err(|e| {
				jsonrpsee::types::ErrorObjectOwned::owned(
					-32602,
					format!("Invalid hex key: {e}"),
					None::<()>,
				)
			})?;

			match self.blockchain.storage(&key_bytes).await {
				Ok(Some(value)) => {
					results.push(ArchiveStorageItem {
						key: item.key,
						value: Some(format!("0x{}", hex::encode(value))),
						hash: None,
					});
				},
				Ok(None) => {
					results.push(ArchiveStorageItem { key: item.key, value: None, hash: None });
				},
				Err(e) => {
					return Err(jsonrpsee::types::ErrorObjectOwned::owned(
						-32603,
						format!("Storage error: {e}"),
						None::<()>,
					));
				},
			}
		}
		Ok(ArchiveStorageResult::OkWithItems { items: results })
	}

	async fn stop_storage(&self, _operation_id: String) -> RpcResult<()> {
		// No-op
		Ok(())
	}

	async fn genesis_hash(&self) -> RpcResult<String> {
		// Return fork point as "genesis" for the forked chain
		let hash = self.blockchain.fork_point();
		Ok(format!("0x{}", hex::encode(hash.as_bytes())))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		ExecutorConfig, TxPool,
		rpc_server::{ForkRpcServer, RpcServerConfig, types::ArchiveStorageResult},
	};
	use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
	use pop_common::test_env::TestNode;
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

	/// Creates a test context with spawned node and RPC server.
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

	/// Transfer amount: 100 units (with 12 decimals)
	const TRANSFER_AMOUNT: u128 = 100_000_000_000_000;

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

	/// Build a mock V4 signed extrinsic with dummy signature (from Alice).
	fn build_mock_signed_extrinsic_v4(call_data: &[u8]) -> Vec<u8> {
		use scale::{Compact, Encode};
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

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_finalized_height_returns_correct_value() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let expected_block_height = ctx.blockchain.head_number().await;

		let height: u32 = client
			.request("archive_v1_finalizedHeight", rpc_params![])
			.await
			.expect("RPC call failed");

		// Height should match the blockchain head number
		assert_eq!(height, expected_block_height);

		// Create a new block
		ctx.blockchain.build_empty_block().await.unwrap();

		let height: u32 = client
			.request("archive_v1_finalizedHeight", rpc_params![])
			.await
			.expect("RPC call failed");

		assert_eq!(height, expected_block_height + 1);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_genesis_hash_returns_valid_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let hash: String = client
			.request("archive_unstable_genesisHash", rpc_params![])
			.await
			.expect("RPC call failed");

		// Hash should be properly formatted
		assert!(hash.starts_with("0x"), "Hash should start with 0x");
		assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");

		// Hash should match fork point
		let expected = format!("0x{}", hex::encode(ctx.blockchain.fork_point().as_bytes()));
		assert_eq!(hash, expected);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_hash_by_height_returns_hash_at_different_heights() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let block_1 = ctx.blockchain.build_empty_block().await.unwrap();
		let block_2 = ctx.blockchain.build_empty_block().await.unwrap();

		let fork_height = ctx.blockchain.fork_point_number();

		// Get hash at fork point height
		let result: Option<Vec<String>> = client
			.request("archive_v1_hashByHeight", rpc_params![fork_height])
			.await
			.expect("RPC call failed");

		let result = result.unwrap();
		assert_eq!(result.len(), 1, "Should return exactly one hash");
		assert!(result[0].starts_with("0x"), "Hash should start with 0x");

		// Hash should match fork point
		let expected = format!("0x{}", hex::encode(ctx.blockchain.fork_point().as_bytes()));
		assert_eq!(result[0], expected);

		// Get hash at further heights
		let result: Option<Vec<String>> = client
			.request("archive_v1_hashByHeight", rpc_params![block_1.number])
			.await
			.expect("RPC call failed");

		let result = result.unwrap();
		assert_eq!(result.len(), 1, "Should return exactly one hash");
		assert!(result[0].starts_with("0x"), "Hash should start with 0x");

		// Hash should match fork point
		let expected = format!("0x{}", hex::encode(block_1.hash.as_bytes()));
		assert_eq!(result[0], expected);

		let result: Option<Vec<String>> = client
			.request("archive_v1_hashByHeight", rpc_params![block_2.number])
			.await
			.expect("RPC call failed");

		let result = result.unwrap();
		assert_eq!(result.len(), 1, "Should return exactly one hash");
		assert!(result[0].starts_with("0x"), "Hash should start with 0x");

		// Hash should match fork point
		let expected = format!("0x{}", hex::encode(block_2.hash.as_bytes()));
		assert_eq!(result[0], expected);

		// Get historical hash (if fork_point isn't 0)
		if fork_height > 0 {
			let result: Option<Vec<String>> = client
				.request("archive_v1_hashByHeight", rpc_params![fork_height - 1])
				.await
				.expect("RPC call failed");

			let result = result.unwrap();
			assert_eq!(result.len(), 1, "Should return exactly one hash");
			assert!(result[0].starts_with("0x"), "Hash should start with 0x");
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_hash_by_height_returns_none_for_unknown_height() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Query a height that doesn't exist (very high number)
		let result: Option<Vec<String>> = client
			.request("archive_v1_hashByHeight", rpc_params![999999999u64])
			.await
			.expect("RPC call failed");

		assert!(result.is_none(), "Should return none array for unknown height");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_header_returns_header_for_head_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		let header: Option<Vec<String>> = client
			.request("archive_unstable_header", rpc_params![head_hash])
			.await
			.expect("RPC call failed");

		assert!(header.is_some(), "Should return header for head hash");
		let header_hex = header.unwrap();
		assert!(header_hex.starts_with(&["0x".to_owned()]), "Header should be hex-encoded");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_header_returns_none_for_unknown_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Use a made-up hash
		let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";

		let header: Option<String> = client
			.request("archive_unstable_header", rpc_params![unknown_hash])
			.await
			.expect("RPC call failed");

		assert!(header.is_none(), "Should return None for unknown hash");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_body_returns_extrinsics_for_valid_hashes() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let fork_point_hash = format!("0x{}", hex::encode(ctx.blockchain.fork_point().0));

		let fork_point_body: Option<Vec<String>> = client
			.request("archive_v1_body", rpc_params![fork_point_hash])
			.await
			.expect("RPC call failed");

		// Build a few blocks
		ctx.blockchain.build_empty_block().await.unwrap();
		ctx.blockchain.build_empty_block().await.unwrap();
		ctx.blockchain.build_empty_block().await.unwrap();

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		let body: Option<Vec<String>> = client
			.request("archive_v1_body", rpc_params![head_hash])
			.await
			.expect("RPC call failed");

		// The latest body is just the mocked timestamp, so should be different from the fork point
		// body
		assert_ne!(fork_point_body.unwrap(), body.unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_body_is_idempotent_over_finalized_blocks() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a few blocks
		ctx.blockchain.build_empty_block().await.unwrap();
		ctx.blockchain.build_empty_block().await.unwrap();
		ctx.blockchain.build_empty_block().await.unwrap();

		let height: u32 = client
			.request("archive_v1_finalizedHeight", rpc_params![])
			.await
			.expect("RPC call failed");

		let hash: Option<Vec<String>> = client
			.request("archive_v1_hashByHeight", rpc_params![height])
			.await
			.expect("RPC call failed");

		let hash = hash.unwrap().pop();

		let body_1: Option<Vec<String>> = client
			.request("archive_v1_body", rpc_params![hash.clone()])
			.await
			.expect("RPC call failed");

		let body_2: Option<Vec<String>> = client
			.request("archive_v1_body", rpc_params![hash])
			.await
			.expect("RPC call failed");

		assert_eq!(body_1, body_2);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_body_returns_none_for_unknown_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";

		let body: Option<Vec<String>> = client
			.request("archive_v1_body", rpc_params![unknown_hash])
			.await
			.expect("RPC call failed");

		assert!(body.is_none(), "Should return None for unknown hash");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_executes_runtime_api() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Call Core_version with empty parameters
		let result: Option<serde_json::Value> = client
			.request("archive_v1_call", rpc_params![head_hash, "Core_version", "0x"])
			.await
			.expect("RPC call failed");

		// Result should be Some (block found)
		let result = result.expect("Should return result for valid block hash");

		// Result should have "success": true with value
		assert_eq!(result.get("success").and_then(|v| v.as_bool()), Some(true));
		let value = result.get("value").and_then(|v| v.as_str());
		assert!(value.is_some(), "Should have value field");
		assert!(value.unwrap().starts_with("0x"), "Value should be hex-encoded");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_returns_error_for_invalid_function() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Call a non-existent function
		let result: Option<serde_json::Value> = client
			.request("archive_v1_call", rpc_params![head_hash, "NonExistent_function", "0x"])
			.await
			.expect("RPC call failed");

		// Result should be Some (block found, but call failed)
		let result = result.expect("Should return result for valid block hash");

		// Result should have "success": false with error message
		assert_eq!(result.get("success").and_then(|v| v.as_bool()), Some(false));
		assert!(result.get("error").is_some(), "Should have error field");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_returns_null_for_unknown_block() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Use a made-up hash that doesn't exist
		let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";

		let result: Option<serde_json::Value> = client
			.request("archive_v1_call", rpc_params![unknown_hash, "Core_version", "0x"])
			.await
			.expect("RPC call failed");

		// Result should be None (block not found)
		assert!(result.is_none(), "Should return null for unknown block hash");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_executes_at_specific_block() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Get fork point hash
		let fork_hash = format!("0x{}", hex::encode(ctx.blockchain.fork_point().as_bytes()));

		// Build a new block so we have multiple blocks
		ctx.blockchain.build_empty_block().await.unwrap();

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Both calls should succeed since both blocks exist
		let result_at_fork: Option<serde_json::Value> = client
			.request("archive_v1_call", rpc_params![fork_hash.clone(), "Core_version", "0x"])
			.await
			.expect("RPC call at fork point failed");

		let result_at_head: Option<serde_json::Value> = client
			.request("archive_v1_call", rpc_params![head_hash, "Core_version", "0x"])
			.await
			.expect("RPC call at head failed");

		// Both should return successful results
		assert!(result_at_fork.is_some(), "Should find fork point block");
		assert!(result_at_head.is_some(), "Should find head block");

		let fork_result = result_at_fork.unwrap();
		let head_result = result_at_head.unwrap();

		assert_eq!(fork_result.get("success").and_then(|v| v.as_bool()), Some(true));
		assert_eq!(head_result.get("success").and_then(|v| v.as_bool()), Some(true));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_rejects_invalid_hex_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Pass invalid hex for hash - this should return a JSON-RPC error
		let result: Result<Option<serde_json::Value>, _> = client
			.request("archive_v1_call", rpc_params!["not_valid_hex", "Core_version", "0x"])
			.await;

		assert!(result.is_err(), "Should reject invalid hex hash");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_returns_value_for_existing_key() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Query System::Number storage key
		let mut key = Vec::new();
		key.extend(sp_core::twox_128(b"System"));
		key.extend(sp_core::twox_128(b"Number"));
		let key_hex = format!("0x{}", hex::encode(&key));

		let items = vec![serde_json::json!({
			"key": key_hex,
			"type": "value"
		})];

		let result: ArchiveStorageResult = client
			.request(
				"archive_unstable_storage",
				rpc_params![head_hash, items, Option::<String>::None],
			)
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageResult::OkWithItems { items } => {
				assert_eq!(items.len(), 1, "Should return one item");
				assert!(items[0].value.is_some(), "Value should be present");
			},
			_ => panic!("Expected OkWithItems result"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_returns_none_for_nonexistent_key() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Query a non-existent key
		let key_hex = format!("0x{}", hex::encode(b"nonexistent_key_12345"));

		let items = vec![serde_json::json!({
			"key": key_hex,
			"type": "value"
		})];

		let result: ArchiveStorageResult = client
			.request(
				"archive_unstable_storage",
				rpc_params![head_hash, items, Option::<String>::None],
			)
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageResult::OkWithItems { items } => {
				assert_eq!(items.len(), 1, "Should return one item");
				assert!(items[0].value.is_none(), "Value should be None for non-existent key");
			},
			_ => panic!("Expected OkWithItems result"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_stop_storage_succeeds() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// stop_storage is a no-op but should succeed
		let result: () = client
			.request("archive_unstable_stopStorage", rpc_params!["some_operation_id"])
			.await
			.expect("RPC call failed");

		// Just verify it doesn't error
		assert_eq!(result, ());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_header_rejects_invalid_hex() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Pass invalid hex
		let result: Result<Option<String>, _> =
			client.request("archive_unstable_header", rpc_params!["not_valid_hex"]).await;

		assert!(result.is_err(), "Should reject invalid hex");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_rejects_invalid_hex_parameters() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Pass invalid hex for call_parameters
		let result: Result<Option<serde_json::Value>, _> = client
			.request("archive_v1_call", rpc_params![head_hash, "Core_version", "not_hex"])
			.await;

		assert!(result.is_err(), "Should reject invalid hex parameters");
	}

	/// Verifies that calling `BlockBuilder_apply_extrinsic` and `BlockBuilder_finalize_block`
	/// via `archive_v1_call` RPC does NOT persist storage changes.
	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_does_not_persist_storage_changes() {
		use crate::SignatureMockMode;
		use scale::{Compact, Encode};

		let config =
			ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
		let ctx = setup_rpc_test_with_config(config).await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Get storage key for Alice
		let alice_key = account_storage_key(&ALICE);

		// Get head block for metadata
		let head = ctx.blockchain.head().await;
		let head_hash = format!("0x{}", hex::encode(head.hash.as_bytes()));
		let metadata = head.metadata().await.expect("Failed to get metadata");

		// Query Alice's balance BEFORE directly via blockchain
		let alice_balance_before = ctx
			.blockchain
			.storage(&alice_key)
			.await
			.expect("Failed to get Alice balance")
			.map(|v| decode_free_balance(&v))
			.expect("Alice should have a balance");

		// Build transfer call data: Balances.transfer_keep_alive(Bob, TRANSFER_AMOUNT)
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

		// Build the signed extrinsic
		let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
		let extrinsic_hex = format!("0x{}", hex::encode(&extrinsic));

		// Call BlockBuilder_apply_extrinsic via archive_v1_call
		let _: Option<serde_json::Value> = client
			.request(
				"archive_v1_call",
				rpc_params![head_hash.clone(), "BlockBuilder_apply_extrinsic", extrinsic_hex],
			)
			.await
			.expect("BlockBuilder_apply_extrinsic RPC call failed");

		// Call BlockBuilder_finalize_block via archive_v1_call
		let _: Option<serde_json::Value> = client
			.request("archive_v1_call", rpc_params![head_hash, "BlockBuilder_finalize_block", "0x"])
			.await
			.expect("BlockBuilder_finalize_block RPC call failed");

		// Query Alice's balance AFTER - should be UNCHANGED
		let alice_balance_after = ctx
			.blockchain
			.storage(&alice_key)
			.await
			.expect("Failed to get Alice balance after")
			.map(|v| decode_free_balance(&v))
			.expect("Alice should still have a balance");

		assert_eq!(
			alice_balance_before, alice_balance_after,
			"Storage should NOT be modified by archive_v1_call even when calling BlockBuilder methods"
		);
	}
}
