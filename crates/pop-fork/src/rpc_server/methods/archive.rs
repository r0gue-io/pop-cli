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
	#[method(name = "v1_call")]
	async fn call(
		&self,
		hash: String,
		function: String,
		call_parameters: String,
	) -> RpcResult<ArchiveCallResult>;

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
		_hash: String,
		function: String,
		call_parameters: String,
	) -> RpcResult<ArchiveCallResult> {
		// Decode parameters
		let params = hex::decode(call_parameters.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex parameters: {e}"),
				None::<()>,
			)
		})?;

		// Execute the call
		match self.blockchain.call(&function, &params).await {
			Ok(result) =>
				Ok(ArchiveCallResult::Ok { output: format!("0x{}", hex::encode(result)) }),
			Err(e) => Ok(ArchiveCallResult::Err { error: e.to_string() }),
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
		TxPool,
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
		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");

		let blockchain =
			Blockchain::fork(&endpoint, None).await.expect("Failed to fork blockchain");
		let txpool = Arc::new(TxPool::new());

		let server = ForkRpcServer::start(blockchain.clone(), txpool, RpcServerConfig::default())
			.await
			.expect("Failed to start RPC server");

		let ws_url = server.ws_url();
		RpcTestContext { node, server, ws_url, blockchain }
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
		let result: serde_json::Value = client
			.request("archive_unstable_call", rpc_params![head_hash, "Core_version", "0x"])
			.await
			.expect("RPC call failed");

		// Result should have "result": "ok" with output
		assert_eq!(result.get("result").and_then(|v| v.as_str()), Some("ok"));
		let output = result.get("output").and_then(|v| v.as_str());
		assert!(output.is_some(), "Should have output field");
		assert!(output.unwrap().starts_with("0x"), "Output should be hex-encoded");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_returns_error_for_invalid_function() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Call a non-existent function
		let result: serde_json::Value = client
			.request("archive_unstable_call", rpc_params![head_hash, "NonExistent_function", "0x"])
			.await
			.expect("RPC call failed");

		// Result should have "result": "err" with error message
		assert_eq!(result.get("result").and_then(|v| v.as_str()), Some("err"));
		assert!(result.get("error").is_some(), "Should have error field");
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
		let result: Result<serde_json::Value, _> = client
			.request("archive_unstable_call", rpc_params![head_hash, "Core_version", "not_hex"])
			.await;

		assert!(result.is_err(), "Should reject invalid hex parameters");
	}
}
