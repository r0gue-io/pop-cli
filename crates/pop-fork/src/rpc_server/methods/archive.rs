// SPDX-License-Identifier: GPL-3.0

//! New archive_v1_* RPC methods.
//!
//! These methods follow the new Substrate JSON-RPC specification for archive nodes.

use crate::rpc_server::types::{ArchiveCallResult, ArchiveStorageItem, ArchiveStorageResult, HashByHeightResult, StorageQueryItem};
use crate::Blockchain;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use std::sync::Arc;

/// New archive RPC methods (v1 spec).
#[rpc(server, namespace = "archive")]
pub trait ArchiveApi {
	/// Get the current finalized block height.
	#[method(name = "unstable_finalizedHeight")]
	async fn finalized_height(&self) -> RpcResult<u64>;

	/// Get block hash by height.
	///
	/// Returns an array of hashes (may be multiple due to forks).
	#[method(name = "unstable_hashByHeight")]
	async fn hash_by_height(&self, height: u64) -> RpcResult<HashByHeightResult>;

	/// Get block header by hash.
	///
	/// Returns hex-encoded SCALE-encoded header.
	#[method(name = "unstable_header")]
	async fn header(&self, hash: String) -> RpcResult<Option<String>>;

	/// Get block body by hash.
	///
	/// Returns array of hex-encoded extrinsics.
	#[method(name = "unstable_body")]
	async fn body(&self, hash: String) -> RpcResult<Option<Vec<String>>>;

	/// Execute a runtime call at a block.
	#[method(name = "unstable_call")]
	async fn call(
		&self,
		hash: String,
		function: String,
		call_parameters: String,
	) -> RpcResult<ArchiveCallResult>;

	/// Query storage at a finalized block.
	#[method(name = "unstable_storage")]
	async fn storage(
		&self,
		hash: String,
		items: Vec<StorageQueryItem>,
		child_trie: Option<String>,
	) -> RpcResult<ArchiveStorageResult>;

	/// Stop a storage query operation.
	#[method(name = "unstable_stopStorage")]
	async fn stop_storage(&self, operation_id: String) -> RpcResult<()>;

	/// Get the genesis hash.
	#[method(name = "unstable_genesisHash")]
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
	async fn finalized_height(&self) -> RpcResult<u64> {
		Ok(self.blockchain.head_number().await as u64)
	}

	async fn hash_by_height(&self, height: u64) -> RpcResult<HashByHeightResult> {
		let fork_point_number = self.blockchain.fork_point_number() as u64;
		let head_number = self.blockchain.head_number().await as u64;

		if height == fork_point_number {
			let hash = self.blockchain.fork_point();
			Ok(HashByHeightResult::Hashes(vec![format!(
				"0x{}",
				hex::encode(hash.as_bytes())
			)]))
		} else if height == head_number {
			let hash = self.blockchain.head_hash().await;
			Ok(HashByHeightResult::Hashes(vec![format!(
				"0x{}",
				hex::encode(hash.as_bytes())
			)]))
		} else {
			// Historical block hashes not available yet
			Ok(HashByHeightResult::Hashes(vec![]))
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

		let head = self.blockchain.head().await;
		let head_hash_bytes = head.hash.as_bytes();

		// Only return body if it matches the current head
		if hash_bytes == head_hash_bytes {
			let extrinsics: Vec<String> =
				head.extrinsics.iter().map(|ext| format!("0x{}", hex::encode(ext))).collect();
			Ok(Some(extrinsics))
		} else {
			Ok(None)
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
			Ok(result) => Ok(ArchiveCallResult::Ok {
				output: format!("0x{}", hex::encode(result)),
			}),
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
