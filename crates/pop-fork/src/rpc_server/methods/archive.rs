// SPDX-License-Identifier: GPL-3.0

//! New archive_v1_* RPC methods.
//!
//! These methods follow the new Substrate JSON-RPC specification for archive nodes.

use super::chain_spec::GENESIS_HASH;
use crate::{
	Blockchain,
	rpc_server::types::{
		ArchiveCallResult, ArchiveStorageDiffResult, ArchiveStorageItem, ArchiveStorageResult,
		StorageDiffItem, StorageDiffQueryItem, StorageDiffType, StorageQueryItem, StorageQueryType,
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
	/// Returns an array of hashes (returns an `Option<Vec>` to comply with the spec but, in
	/// practice, this Vec always contains a single element, as blocks are produced on-demand one
	/// by one).
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

	/// Get the genesis hash.
	#[method(name = "v1_genesisHash")]
	async fn genesis_hash(&self) -> RpcResult<String>;

	/// Query storage differences between two blocks for specific keys.
	///
	/// This is a simplified implementation for fork nodes that does NOT support:
	/// - Iterating all keys (items parameter is required)
	/// - Child trie queries
	///
	/// Only keys that have changed between the two blocks are returned.
	///
	/// If `previous_hash` is not provided, compares against the parent block.
	#[method(name = "v1_storageDiff")]
	async fn storage_diff(
		&self,
		hash: String,
		items: Vec<StorageDiffQueryItem>,
		previous_hash: Option<String>,
	) -> RpcResult<ArchiveStorageDiffResult>;
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

		// Convert to H256
		let block_hash = H256::from_slice(&hash_bytes);

		// Fetch block header (checks local blocks first, then remote)
		match self.blockchain.block_header(block_hash).await {
			Ok(Some(header)) => Ok(Some(format!("0x{}", hex::encode(&header)))),
			Ok(None) => Ok(None),
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to fetch block header: {e}"),
				None::<()>,
			)),
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
		hash: String,
		items: Vec<StorageQueryItem>,
		_child_trie: Option<String>,
	) -> RpcResult<ArchiveStorageResult> {
		// Parse and validate hash
		let hash_bytes = hex::decode(hash.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex hash: {e}"),
				None::<()>,
			)
		})?;
		let block_hash = H256::from_slice(&hash_bytes);

		// Get block number from hash
		let block_number = match self.blockchain.block_number_by_hash(block_hash).await {
			Ok(Some(num)) => num,
			Ok(None) =>
				return Ok(ArchiveStorageResult::Err { error: "Block not found".to_string() }),
			Err(e) =>
				return Err(jsonrpsee::types::ErrorObjectOwned::owned(
					-32603,
					format!("Failed to resolve block: {e}"),
					None::<()>,
				)),
		};

		// Query storage for each item at the specific block
		let mut results = Vec::new();
		for item in items {
			let key_bytes = hex::decode(item.key.trim_start_matches("0x")).map_err(|e| {
				jsonrpsee::types::ErrorObjectOwned::owned(
					-32602,
					format!("Invalid hex key: {e}"),
					None::<()>,
				)
			})?;

			match self.blockchain.storage_at(block_number, &key_bytes).await {
				Ok(Some(value)) => match item.query_type {
					StorageQueryType::Value => {
						results.push(ArchiveStorageItem {
							key: item.key,
							value: Some(format!("0x{}", hex::encode(&value))),
							hash: None,
						});
					},
					StorageQueryType::Hash => {
						let hash = sp_core::blake2_256(&value);
						results.push(ArchiveStorageItem {
							key: item.key,
							value: None,
							hash: Some(format!("0x{}", hex::encode(hash))),
						});
					},
				},
				Ok(None) => {
					// Key doesn't exist - include in results with null value
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
		Ok(ArchiveStorageResult::Ok { items: results })
	}

	async fn genesis_hash(&self) -> RpcResult<String> {
		// Return cached value if available (shared with chainSpec)
		if let Some(hash) = GENESIS_HASH.get() {
			return Ok(hash.clone());
		}

		// Fetch genesis hash (block 0) and cache it
		match self.blockchain.block_hash_at(0).await {
			Ok(Some(hash)) => {
				let formatted = format!("0x{}", hex::encode(hash.as_bytes()));
				Ok(GENESIS_HASH.get_or_init(|| formatted).clone())
			},
			Ok(None) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				"Genesis block not found",
				None::<()>,
			)),
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to fetch genesis hash: {e}"),
				None::<()>,
			)),
		}
	}

	async fn storage_diff(
		&self,
		hash: String,
		items: Vec<StorageDiffQueryItem>,
		previous_hash: Option<String>,
	) -> RpcResult<ArchiveStorageDiffResult> {
		// Parse and validate hash
		let hash_bytes = hex::decode(hash.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex hash: {e}"),
				None::<()>,
			)
		})?;
		let block_hash = H256::from_slice(&hash_bytes);

		// Get block number for the target block
		let block_number = match self.blockchain.block_number_by_hash(block_hash).await {
			Ok(Some(num)) => num,
			Ok(None) =>
				return Ok(ArchiveStorageDiffResult::Err { error: "Block not found".to_string() }),
			Err(e) =>
				return Err(jsonrpsee::types::ErrorObjectOwned::owned(
					-32603,
					format!("Failed to resolve block: {e}"),
					None::<()>,
				)),
		};

		// Determine the previous block hash
		let prev_block_hash = match previous_hash {
			Some(prev_hash_str) => {
				// Parse provided previous hash
				let prev_hash_bytes =
					hex::decode(prev_hash_str.trim_start_matches("0x")).map_err(|e| {
						jsonrpsee::types::ErrorObjectOwned::owned(
							-32602,
							format!("Invalid hex previousHash: {e}"),
							None::<()>,
						)
					})?;
				H256::from_slice(&prev_hash_bytes)
			},
			None => {
				// Get parent hash from the block
				match self.blockchain.block_parent_hash(block_hash).await {
					Ok(Some(parent_hash)) => parent_hash,
					Ok(None) =>
						return Ok(ArchiveStorageDiffResult::Err {
							error: "Block not found".to_string(),
						}),
					Err(e) =>
						return Err(jsonrpsee::types::ErrorObjectOwned::owned(
							-32603,
							format!("Failed to get parent hash: {e}"),
							None::<()>,
						)),
				}
			},
		};

		// Get block number for the previous block
		let prev_block_number = match self.blockchain.block_number_by_hash(prev_block_hash).await {
			Ok(Some(num)) => num,
			Ok(None) =>
				return Ok(ArchiveStorageDiffResult::Err {
					error: "Previous block not found".to_string(),
				}),
			Err(e) =>
				return Err(jsonrpsee::types::ErrorObjectOwned::owned(
					-32603,
					format!("Failed to resolve previous block: {e}"),
					None::<()>,
				)),
		};

		// Query storage for each item at both blocks and compute differences
		let mut results = Vec::new();
		for item in items {
			let key_bytes = hex::decode(item.key.trim_start_matches("0x")).map_err(|e| {
				jsonrpsee::types::ErrorObjectOwned::owned(
					-32602,
					format!("Invalid hex key: {e}"),
					None::<()>,
				)
			})?;

			// Get value at current block
			let current_value = match self.blockchain.storage_at(block_number, &key_bytes).await {
				Ok(v) => v,
				Err(e) => {
					return Err(jsonrpsee::types::ErrorObjectOwned::owned(
						-32603,
						format!("Storage error: {e}"),
						None::<()>,
					));
				},
			};

			// Get value at previous block
			let previous_value =
				match self.blockchain.storage_at(prev_block_number, &key_bytes).await {
					Ok(v) => v,
					Err(e) => {
						return Err(jsonrpsee::types::ErrorObjectOwned::owned(
							-32603,
							format!("Storage error: {e}"),
							None::<()>,
						));
					},
				};

			// Determine diff type and build result
			let diff_item = match (&current_value, &previous_value) {
				// Both None - no change, skip
				(None, None) => continue,

				// Added: exists in current but not in previous
				(Some(value), None) => {
					let (value_field, hash_field) = match item.return_type {
						StorageQueryType::Value =>
							(Some(format!("0x{}", hex::encode(value))), None),
						StorageQueryType::Hash =>
							(None, Some(format!("0x{}", hex::encode(sp_core::blake2_256(value))))),
					};
					StorageDiffItem {
						key: item.key,
						value: value_field,
						hash: hash_field,
						diff_type: StorageDiffType::Added,
					}
				},

				// Deleted: exists in previous but not in current
				(None, Some(_)) => {
					// For deleted items, we don't return value/hash (the key no longer exists)
					StorageDiffItem {
						key: item.key,
						value: None,
						hash: None,
						diff_type: StorageDiffType::Deleted,
					}
				},

				// Both exist - check if modified
				(Some(curr), Some(prev)) => {
					if curr == prev {
						// No change, skip
						continue;
					}
					// Modified
					let (value_field, hash_field) = match item.return_type {
						StorageQueryType::Value => (Some(format!("0x{}", hex::encode(curr))), None),
						StorageQueryType::Hash =>
							(None, Some(format!("0x{}", hex::encode(sp_core::blake2_256(curr))))),
					};
					StorageDiffItem {
						key: item.key,
						value: value_field,
						hash: hash_field,
						diff_type: StorageDiffType::Modified,
					}
				},
			};

			results.push(diff_item);
		}

		Ok(ArchiveStorageDiffResult::Ok { items: results })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		ExecutorConfig, TxPool,
		rpc_server::{
			ForkRpcServer, RpcServerConfig,
			types::{ArchiveCallResult, ArchiveStorageResult},
		},
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
			.request("archive_v1_genesisHash", rpc_params![])
			.await
			.expect("RPC call failed");

		// Hash should be properly formatted
		assert!(hash.starts_with("0x"), "Hash should start with 0x");
		assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");

		// Hash should match the actual genesis hash (block 0)
		let expected_hash = ctx
			.blockchain
			.block_hash_at(0)
			.await
			.expect("Failed to get genesis hash")
			.expect("Genesis block should exist");
		let expected = format!("0x{}", hex::encode(expected_hash.as_bytes()));
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

		// Build a block so we have a locally-built header
		ctx.blockchain.build_empty_block().await.unwrap();

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		let header: Option<String> = client
			.request("archive_v1_header", rpc_params![head_hash])
			.await
			.expect("RPC call failed");

		assert!(header.is_some(), "Should return header for head hash");
		let header_hex = header.unwrap();
		assert!(header_hex.starts_with("0x"), "Header should be hex-encoded");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_header_returns_none_for_unknown_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Use a made-up hash
		let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";

		let header: Option<String> = client
			.request("archive_v1_header", rpc_params![unknown_hash])
			.await
			.expect("RPC call failed");

		assert!(header.is_none(), "Should return None for unknown hash");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_header_returns_header_for_fork_point() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let fork_point_hash = format!("0x{}", hex::encode(ctx.blockchain.fork_point().0));

		let header: Option<String> = client
			.request("archive_v1_header", rpc_params![fork_point_hash])
			.await
			.expect("RPC call failed");

		assert!(header.is_some(), "Should return header for fork point");
		let header_hex = header.unwrap();
		assert!(header_hex.starts_with("0x"), "Header should be hex-encoded");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_header_returns_header_for_parent_block() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build two blocks
		let block1 = ctx.blockchain.build_empty_block().await.unwrap();
		let _block2 = ctx.blockchain.build_empty_block().await.unwrap();

		let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

		let header: Option<String> = client
			.request("archive_v1_header", rpc_params![block1_hash])
			.await
			.expect("RPC call failed");

		assert!(header.is_some(), "Should return header for parent block");
		let header_hex = header.unwrap();
		assert!(header_hex.starts_with("0x"), "Header should be hex-encoded");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_header_is_idempotent_over_finalized_blocks() {
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

		let header_1: Option<String> = client
			.request("archive_v1_header", rpc_params![hash.clone()])
			.await
			.expect("RPC call failed");

		let header_2: Option<String> = client
			.request("archive_v1_header", rpc_params![hash])
			.await
			.expect("RPC call failed");

		assert_eq!(header_1, header_2, "Header should be idempotent");
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
			.request("archive_v1_storage", rpc_params![head_hash, items, Option::<String>::None])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageResult::Ok { items } => {
				assert_eq!(items.len(), 1, "Should return one item");
				assert!(items[0].value.is_some(), "Value should be present");
			},
			_ => panic!("Expected Ok result"),
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
			.request("archive_v1_storage", rpc_params![head_hash, items, Option::<String>::None])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageResult::Ok { items } => {
				assert_eq!(items.len(), 1, "Should return one item");
				assert!(items[0].value.is_none(), "Value should be None for non-existent key");
			},
			_ => panic!("Expected Ok result"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_header_rejects_invalid_hex() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Pass invalid hex
		let result: Result<Option<String>, _> =
			client.request("archive_v1_header", rpc_params!["not_valid_hex"]).await;

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

	/// Verifies that calling `Core_initialize_block` via `archive_v1_call` RPC does NOT
	/// persist storage changes.
	///
	/// `Core_initialize_block` writes to `System::Number` and other storage keys during
	/// block initialization. This test verifies those changes are discarded after the call.
	#[tokio::test(flavor = "multi_thread")]
	async fn archive_call_does_not_persist_storage_changes() {
		use crate::{DigestItem, consensus_engine, create_next_header};

		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Get head block info
		let head = ctx.blockchain.head().await;
		let head_hash = format!("0x{}", hex::encode(head.hash.as_bytes()));
		let head_number = head.number;

		// System::Number storage key = twox128("System") ++ twox128("Number")
		let system_number_key: Vec<u8> =
			[sp_core::twox_128(b"System").as_slice(), sp_core::twox_128(b"Number").as_slice()]
				.concat();

		// Query System::Number BEFORE
		let number_before = ctx
			.blockchain
			.storage(&system_number_key)
			.await
			.expect("Failed to get System::Number")
			.map(|v| u32::from_le_bytes(v.try_into().expect("System::Number should be 4 bytes")))
			.expect("System::Number should exist");

		// Build header for the next block using the crate's helper
		let header = create_next_header(
			&head,
			vec![DigestItem::PreRuntime(consensus_engine::AURA, 0u64.to_le_bytes().to_vec())],
		);
		let header_hex = format!("0x{}", hex::encode(&header));

		// Call Core_initialize_block - this WOULD write System::Number = head_number + 1
		let init_result: Option<ArchiveCallResult> = client
			.request("archive_v1_call", rpc_params![head_hash, "Core_initialize_block", header_hex])
			.await
			.expect("Core_initialize_block RPC call failed");
		let init_result = init_result.expect("Block should exist");
		assert!(
			init_result.success,
			"Core_initialize_block should succeed: {:?}",
			init_result.error
		);

		// Query System::Number AFTER - should be UNCHANGED
		let number_after = ctx
			.blockchain
			.storage(&system_number_key)
			.await
			.expect("Failed to get System::Number after")
			.map(|v| u32::from_le_bytes(v.try_into().expect("System::Number should be 4 bytes")))
			.expect("System::Number should still exist");

		assert_eq!(
			number_before,
			number_after,
			"System::Number should NOT be modified by archive_v1_call. \
			 Before: {}, After: {} (would have been {} if persisted)",
			number_before,
			number_after,
			head_number + 1
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_returns_hash_when_requested() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let head_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Query System::Number storage key with hash type
		let mut key = Vec::new();
		key.extend(sp_core::twox_128(b"System"));
		key.extend(sp_core::twox_128(b"Number"));
		let key_hex = format!("0x{}", hex::encode(&key));

		let items = vec![serde_json::json!({
			"key": key_hex,
			"type": "hash"
		})];

		let result: ArchiveStorageResult = client
			.request("archive_v1_storage", rpc_params![head_hash, items, Option::<String>::None])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageResult::Ok { items } => {
				assert_eq!(items.len(), 1, "Should return one item");
				assert!(items[0].hash.is_some(), "Hash should be present");
				assert!(items[0].value.is_none(), "Value should not be present");
				let hash = items[0].hash.as_ref().unwrap();
				assert!(hash.starts_with("0x"), "Hash should be hex-encoded");
				assert_eq!(hash.len(), 66, "Hash should be 32 bytes (0x + 64 hex chars)");
			},
			_ => panic!("Expected Ok result"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_queries_at_specific_block() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a block to change state
		ctx.blockchain.build_empty_block().await.unwrap();
		let block1_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Build another block
		ctx.blockchain.build_empty_block().await.unwrap();
		let block2_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));

		// Query System::Number at both blocks
		let mut key = Vec::new();
		key.extend(sp_core::twox_128(b"System"));
		key.extend(sp_core::twox_128(b"Number"));
		let key_hex = format!("0x{}", hex::encode(&key));

		let items = vec![serde_json::json!({ "key": key_hex, "type": "value" })];

		let result1: ArchiveStorageResult = client
			.request(
				"archive_v1_storage",
				rpc_params![block1_hash, items.clone(), Option::<String>::None],
			)
			.await
			.expect("RPC call failed");

		let result2: ArchiveStorageResult = client
			.request("archive_v1_storage", rpc_params![block2_hash, items, Option::<String>::None])
			.await
			.expect("RPC call failed");

		// The block numbers should be different
		match (result1, result2) {
			(
				ArchiveStorageResult::Ok { items: items1 },
				ArchiveStorageResult::Ok { items: items2 },
			) => {
				assert_ne!(items1[0].value, items2[0].value, "Block numbers should differ");
			},
			_ => panic!("Expected Ok results"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_returns_error_for_unknown_block() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
		let items = vec![serde_json::json!({ "key": "0x1234", "type": "value" })];

		let result: ArchiveStorageResult = client
			.request("archive_v1_storage", rpc_params![unknown_hash, items, Option::<String>::None])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageResult::Err { error } => {
				assert!(
					error.contains("not found") || error.contains("Block"),
					"Should indicate block not found"
				);
			},
			_ => panic!("Expected Err result for unknown block"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_detects_modified_value() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Create a test key
		let test_key = b"test_storage_diff_key";
		let test_key_hex = format!("0x{}", hex::encode(test_key));

		// Set initial value and build first block
		ctx.blockchain.set_storage_for_testing(test_key, Some(b"value1")).await;
		let block1 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

		// Set modified value and build second block
		ctx.blockchain.set_storage_for_testing(test_key, Some(b"value2")).await;
		let block2 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));

		// Query storage diff
		let items = vec![serde_json::json!({
			"key": test_key_hex,
			"returnType": "value"
		})];

		let result: ArchiveStorageDiffResult = client
			.request("archive_v1_storageDiff", rpc_params![block2_hash, items, block1_hash])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Ok { items } => {
				assert_eq!(items.len(), 1, "Should return one modified item");
				assert_eq!(items[0].key, test_key_hex);
				assert_eq!(items[0].diff_type, StorageDiffType::Modified);
				assert!(items[0].value.is_some(), "Value should be present");
				assert_eq!(
					items[0].value.as_ref().unwrap(),
					&format!("0x{}", hex::encode(b"value2"))
				);
			},
			ArchiveStorageDiffResult::Err { error } =>
				panic!("Expected Ok result, got error: {error}"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_returns_empty_for_unchanged_keys() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Create a test key with value that won't change
		let test_key = b"test_unchanged_key";
		let test_key_hex = format!("0x{}", hex::encode(test_key));

		// Set value and build first block
		ctx.blockchain.set_storage_for_testing(test_key, Some(b"constant_value")).await;
		let block1 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

		// Build second block without changing the value
		let block2 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));

		// Query storage diff
		let items = vec![serde_json::json!({
			"key": test_key_hex,
			"returnType": "value"
		})];

		let result: ArchiveStorageDiffResult = client
			.request("archive_v1_storageDiff", rpc_params![block2_hash, items, block1_hash])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Ok { items } => {
				assert!(items.is_empty(), "Should return empty for unchanged keys");
			},
			ArchiveStorageDiffResult::Err { error } =>
				panic!("Expected Ok result, got error: {error}"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_returns_added_for_new_key() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build first block without the key
		let block1 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

		// Add a new key and build second block
		let test_key = b"test_added_key";
		let test_key_hex = format!("0x{}", hex::encode(test_key));
		ctx.blockchain.set_storage_for_testing(test_key, Some(b"new_value")).await;
		let block2 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));

		// Query storage diff
		let items = vec![serde_json::json!({
			"key": test_key_hex,
			"returnType": "value"
		})];

		let result: ArchiveStorageDiffResult = client
			.request("archive_v1_storageDiff", rpc_params![block2_hash, items, block1_hash])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Ok { items } => {
				assert_eq!(items.len(), 1, "Should return one added item");
				assert_eq!(items[0].key, test_key_hex);
				assert_eq!(items[0].diff_type, StorageDiffType::Added);
				assert!(items[0].value.is_some(), "Value should be present");
				assert_eq!(
					items[0].value.as_ref().unwrap(),
					&format!("0x{}", hex::encode(b"new_value"))
				);
			},
			ArchiveStorageDiffResult::Err { error } =>
				panic!("Expected Ok result, got error: {error}"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_returns_deleted_for_removed_key() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Add a key and build first block
		let test_key = b"test_deleted_key";
		let test_key_hex = format!("0x{}", hex::encode(test_key));
		ctx.blockchain.set_storage_for_testing(test_key, Some(b"will_be_deleted")).await;
		let block1 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

		// Delete the key and build second block
		ctx.blockchain.set_storage_for_testing(test_key, None).await;
		let block2 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));

		// Query storage diff
		let items = vec![serde_json::json!({
			"key": test_key_hex,
			"returnType": "value"
		})];

		let result: ArchiveStorageDiffResult = client
			.request("archive_v1_storageDiff", rpc_params![block2_hash, items, block1_hash])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Ok { items } => {
				assert_eq!(items.len(), 1, "Should return one deleted item");
				assert_eq!(items[0].key, test_key_hex);
				assert_eq!(items[0].diff_type, StorageDiffType::Deleted);
				assert!(items[0].value.is_none(), "Value should be None for deleted key");
				assert!(items[0].hash.is_none(), "Hash should be None for deleted key");
			},
			ArchiveStorageDiffResult::Err { error } =>
				panic!("Expected Ok result, got error: {error}"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_returns_hash_when_requested() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Create a test key
		let test_key = b"test_hash_key";
		let test_key_hex = format!("0x{}", hex::encode(test_key));

		// Set initial value and build first block
		ctx.blockchain.set_storage_for_testing(test_key, Some(b"value1")).await;
		let block1 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

		// Set modified value and build second block
		let new_value = b"value2";
		ctx.blockchain.set_storage_for_testing(test_key, Some(new_value)).await;
		let block2 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));

		// Query storage diff with hash returnType
		let items = vec![serde_json::json!({
			"key": test_key_hex,
			"returnType": "hash"
		})];

		let result: ArchiveStorageDiffResult = client
			.request("archive_v1_storageDiff", rpc_params![block2_hash, items, block1_hash])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Ok { items } => {
				assert_eq!(items.len(), 1, "Should return one modified item");
				assert_eq!(items[0].diff_type, StorageDiffType::Modified);
				assert!(items[0].value.is_none(), "Value should not be present");
				assert!(items[0].hash.is_some(), "Hash should be present");
				let expected_hash = format!("0x{}", hex::encode(sp_core::blake2_256(new_value)));
				assert_eq!(items[0].hash.as_ref().unwrap(), &expected_hash);
			},
			ArchiveStorageDiffResult::Err { error } =>
				panic!("Expected Ok result, got error: {error}"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_returns_error_for_unknown_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
		let valid_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));
		let items = vec![serde_json::json!({ "key": "0x1234", "returnType": "value" })];

		let result: ArchiveStorageDiffResult = client
			.request("archive_v1_storageDiff", rpc_params![unknown_hash, items, valid_hash])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Err { error } => {
				assert!(
					error.contains("not found") || error.contains("Block"),
					"Should indicate block not found"
				);
			},
			_ => panic!("Expected Err result for unknown block"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_returns_error_for_unknown_previous_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let valid_hash = format!("0x{}", hex::encode(ctx.blockchain.head_hash().await.as_bytes()));
		let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
		let items = vec![serde_json::json!({ "key": "0x1234", "returnType": "value" })];

		let result: ArchiveStorageDiffResult = client
			.request("archive_v1_storageDiff", rpc_params![valid_hash, items, unknown_hash])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Err { error } => {
				assert!(
					error.contains("not found") || error.contains("Previous block"),
					"Should indicate previous block not found"
				);
			},
			_ => panic!("Expected Err result for unknown previous block"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_uses_parent_when_previous_hash_omitted() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Create a test key
		let test_key = b"test_parent_key";
		let test_key_hex = format!("0x{}", hex::encode(test_key));

		// Set initial value and build first block (parent)
		ctx.blockchain.set_storage_for_testing(test_key, Some(b"parent_value")).await;
		ctx.blockchain.build_empty_block().await.expect("Failed to build block");

		// Set modified value and build second block (child)
		ctx.blockchain.set_storage_for_testing(test_key, Some(b"child_value")).await;

		let child_block = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let child_hash = format!("0x{}", hex::encode(child_block.hash.as_bytes()));

		// Query storage diff without previous_hash (should use parent)
		let items = vec![serde_json::json!({
			"key": test_key_hex,
			"returnType": "value"
		})];

		let result: ArchiveStorageDiffResult = client
			.request(
				"archive_v1_storageDiff",
				rpc_params![child_hash, items, Option::<String>::None],
			)
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Ok { items } => {
				assert_eq!(items.len(), 1, "Should return one modified item");
				assert_eq!(items[0].diff_type, StorageDiffType::Modified);
				assert_eq!(
					items[0].value.as_ref().unwrap(),
					&format!("0x{}", hex::encode(b"child_value"))
				);
			},
			ArchiveStorageDiffResult::Err { error } =>
				panic!("Expected Ok result, got error: {error}"),
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn archive_storage_diff_handles_multiple_items() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Create test keys
		let added_key = b"test_multi_added";
		let modified_key = b"test_multi_modified";
		let deleted_key = b"test_multi_deleted";
		let unchanged_key = b"test_multi_unchanged";

		// Set up initial state for block 1
		ctx.blockchain.set_storage_for_testing(modified_key, Some(b"old_value")).await;
		ctx.blockchain.set_storage_for_testing(deleted_key, Some(b"to_delete")).await;
		ctx.blockchain.set_storage_for_testing(unchanged_key, Some(b"constant")).await;
		let block1 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

		// Modify state for block 2
		ctx.blockchain.set_storage_for_testing(added_key, Some(b"new_key")).await;
		ctx.blockchain.set_storage_for_testing(modified_key, Some(b"new_value")).await;
		ctx.blockchain.set_storage_for_testing(deleted_key, None).await;
		// unchanged_key stays the same
		let block2 = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));

		// Query storage diff for all keys
		let items = vec![
			serde_json::json!({ "key": format!("0x{}", hex::encode(added_key)), "returnType": "value" }),
			serde_json::json!({ "key": format!("0x{}", hex::encode(modified_key)), "returnType": "value" }),
			serde_json::json!({ "key": format!("0x{}", hex::encode(deleted_key)), "returnType": "value" }),
			serde_json::json!({ "key": format!("0x{}", hex::encode(unchanged_key)), "returnType": "value" }),
		];

		let result: ArchiveStorageDiffResult = client
			.request("archive_v1_storageDiff", rpc_params![block2_hash, items, block1_hash])
			.await
			.expect("RPC call failed");

		match result {
			ArchiveStorageDiffResult::Ok { items } => {
				// Should have 3 items (added, modified, deleted) but NOT unchanged
				assert_eq!(items.len(), 3, "Should return 3 changed items (not unchanged)");

				// Find each item by key
				let added = items.iter().find(|i| i.key == format!("0x{}", hex::encode(added_key)));
				let modified =
					items.iter().find(|i| i.key == format!("0x{}", hex::encode(modified_key)));
				let deleted =
					items.iter().find(|i| i.key == format!("0x{}", hex::encode(deleted_key)));
				let unchanged =
					items.iter().find(|i| i.key == format!("0x{}", hex::encode(unchanged_key)));

				assert!(added.is_some(), "Added key should be in results");
				assert_eq!(added.unwrap().diff_type, StorageDiffType::Added);

				assert!(modified.is_some(), "Modified key should be in results");
				assert_eq!(modified.unwrap().diff_type, StorageDiffType::Modified);

				assert!(deleted.is_some(), "Deleted key should be in results");
				assert_eq!(deleted.unwrap().diff_type, StorageDiffType::Deleted);

				assert!(unchanged.is_none(), "Unchanged key should NOT be in results");
			},
			ArchiveStorageDiffResult::Err { error } =>
				panic!("Expected Ok result, got error: {error}"),
		}
	}
}
