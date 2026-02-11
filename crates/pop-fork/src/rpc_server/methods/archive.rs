// SPDX-License-Identifier: GPL-3.0

//! New archive_v1_* RPC methods.
//!
//! These methods follow the new Substrate JSON-RPC specification for archive nodes.

use crate::{
	Blockchain,
	rpc_server::{
		RpcServerError, parse_block_hash, parse_hex_bytes,
		types::{
			ArchiveCallResult, ArchiveStorageDiffResult, ArchiveStorageItem, ArchiveStorageResult,
			HexString, StorageDiffItem, StorageDiffQueryItem, StorageDiffType, StorageQueryItem,
			StorageQueryType,
		},
	},
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc, tracing};
use std::sync::Arc;

#[async_trait::async_trait]
pub trait ArchiveBlockchain: Send + Sync {
	async fn head_number(&self) -> u32;
	async fn block_hash_at(
		&self,
		height: u32,
	) -> Result<Option<subxt::utils::H256>, crate::BlockchainError>;
	async fn block_header(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<Vec<u8>>, crate::BlockchainError>;
	async fn block_body(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<Vec<Vec<u8>>>, crate::BlockchainError>;
	async fn call_at_block(
		&self,
		hash: subxt::utils::H256,
		function: &str,
		params: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError>;
	async fn block_number_by_hash(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<u32>, crate::BlockchainError>;
	async fn storage_at(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError>;
	async fn storage_keys_by_prefix(
		&self,
		prefix: &[u8],
		at: subxt::utils::H256,
	) -> Result<Vec<Vec<u8>>, crate::BlockchainError>;
	async fn genesis_hash(&self) -> Result<String, crate::BlockchainError>;
	async fn block_parent_hash(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<subxt::utils::H256>, crate::BlockchainError>;
}

#[async_trait::async_trait]
impl ArchiveBlockchain for Blockchain {
	async fn head_number(&self) -> u32 {
		Blockchain::head_number(self).await
	}

	async fn block_hash_at(
		&self,
		height: u32,
	) -> Result<Option<subxt::utils::H256>, crate::BlockchainError> {
		Blockchain::block_hash_at(self, height).await
	}

	async fn block_header(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
		Blockchain::block_header(self, hash).await
	}

	async fn block_body(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<Vec<Vec<u8>>>, crate::BlockchainError> {
		Blockchain::block_body(self, hash).await
	}

	async fn call_at_block(
		&self,
		hash: subxt::utils::H256,
		function: &str,
		params: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
		Blockchain::call_at_block(self, hash, function, params).await
	}

	async fn block_number_by_hash(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<u32>, crate::BlockchainError> {
		Blockchain::block_number_by_hash(self, hash).await
	}

	async fn storage_at(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
		Blockchain::storage_at(self, block_number, key).await
	}

	async fn storage_keys_by_prefix(
		&self,
		prefix: &[u8],
		at: subxt::utils::H256,
	) -> Result<Vec<Vec<u8>>, crate::BlockchainError> {
		Blockchain::storage_keys_by_prefix(self, prefix, at).await
	}

	async fn genesis_hash(&self) -> Result<String, crate::BlockchainError> {
		Blockchain::genesis_hash(self).await
	}

	async fn block_parent_hash(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<subxt::utils::H256>, crate::BlockchainError> {
		Blockchain::block_parent_hash(self, hash).await
	}
}

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
pub struct ArchiveApi<T: ArchiveBlockchain = Blockchain> {
	blockchain: Arc<T>,
}

impl<T: ArchiveBlockchain> ArchiveApi<T> {
	/// Create a new ArchiveApi instance.
	pub fn new(blockchain: Arc<T>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl<T: ArchiveBlockchain + 'static> ArchiveApiServer for ArchiveApi<T> {
	async fn finalized_height(&self) -> RpcResult<u32> {
		Ok(self.blockchain.head_number().await)
	}

	async fn hash_by_height(&self, height: u32) -> RpcResult<Option<Vec<String>>> {
		// Fetch block hash (checks local blocks first, then remote)
		match self.blockchain.block_hash_at(height).await {
			Ok(Some(hash)) => Ok(Some(vec![HexString::from_bytes(hash.as_bytes()).into()])),
			Ok(None) => Ok(None),
			Err(e) =>
				Err(RpcServerError::Internal(format!("Failed to fetch block hash: {e}")).into()),
		}
	}

	async fn header(&self, hash: String) -> RpcResult<Option<String>> {
		let block_hash = parse_block_hash(&hash)?;

		// Fetch block header (checks local blocks first, then remote)
		match self.blockchain.block_header(block_hash).await {
			Ok(Some(header)) => Ok(Some(HexString::from_bytes(&header).into())),
			Ok(None) => Ok(None),
			Err(e) =>
				Err(RpcServerError::Internal(format!("Failed to fetch block header: {e}")).into()),
		}
	}

	async fn body(&self, hash: String) -> RpcResult<Option<Vec<String>>> {
		let block_hash = parse_block_hash(&hash)?;

		// Fetch block body (checks local blocks first, then remote)
		match self.blockchain.block_body(block_hash).await {
			Ok(Some(extrinsics)) => {
				let hex_extrinsics: Vec<String> =
					extrinsics.iter().map(|ext| HexString::from_bytes(ext).into()).collect();
				Ok(Some(hex_extrinsics))
			},
			Ok(None) => Ok(None),
			Err(e) =>
				Err(RpcServerError::Internal(format!("Failed to fetch block body: {e}")).into()),
		}
	}

	async fn call(
		&self,
		hash: String,
		function: String,
		call_parameters: String,
	) -> RpcResult<Option<ArchiveCallResult>> {
		let block_hash = parse_block_hash(&hash)?;
		let params = parse_hex_bytes(&call_parameters, "parameters")?;

		// Execute the call at the specified block
		match self.blockchain.call_at_block(block_hash, &function, &params).await {
			Ok(Some(result)) =>
				Ok(Some(ArchiveCallResult::ok(HexString::from_bytes(&result).into()))),
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
		let block_hash = parse_block_hash(&hash)?;

		// Get block number from hash
		let block_number = match self.blockchain.block_number_by_hash(block_hash).await {
			Ok(Some(num)) => num,
			Ok(None) => {
				return Ok(ArchiveStorageResult::Err { error: "Block not found".to_string() });
			},
			Err(e) => {
				return Err(RpcServerError::Internal(format!("Failed to resolve block: {e}")).into());
			},
		};

		// Query storage for each item at the specific block
		let mut results = Vec::new();
		for item in items {
			let key_bytes = parse_hex_bytes(&item.key, "key")?;

			match item.query_type {
				StorageQueryType::ClosestDescendantMerkleValue => {
					// Merkle proofs not supported in fork - return empty result
					results.push(ArchiveStorageItem { key: item.key, value: None, hash: None });
					continue;
				},
				StorageQueryType::DescendantsValues => {
					tracing::debug!(
						prefix = %item.key,
						"archive_v1_storage: DescendantsValues query"
					);
					match self.blockchain.storage_keys_by_prefix(&key_bytes, block_hash).await {
						Ok(keys) => {
							tracing::debug!(
								prefix = %item.key,
								keys_found = keys.len(),
								"archive_v1_storage: DescendantsValues fetching values in parallel"
							);
							let futs: Vec<_> = keys
								.iter()
								.map(|k| self.blockchain.storage_at(block_number, k))
								.collect();
							let values = futures::future::join_all(futs).await;
							for (k, v) in keys.into_iter().zip(values) {
								let value = match v {
									Ok(Some(val)) => Some(HexString::from_bytes(&val).into()),
									_ => None,
								};
								results.push(ArchiveStorageItem {
									key: HexString::from_bytes(&k).into(),
									value,
									hash: None,
								});
							}
						},
						Err(e) => {
							tracing::debug!(
								prefix = %item.key,
								error = %e,
								"archive_v1_storage: DescendantsValues prefix lookup failed"
							);
						},
					}
					continue;
				},
				StorageQueryType::DescendantsHashes => {
					tracing::debug!(
						prefix = %item.key,
						"archive_v1_storage: DescendantsHashes query"
					);
					match self.blockchain.storage_keys_by_prefix(&key_bytes, block_hash).await {
						Ok(keys) => {
							tracing::debug!(
								prefix = %item.key,
								keys_found = keys.len(),
								"archive_v1_storage: DescendantsHashes fetching values in parallel"
							);
							let futs: Vec<_> = keys
								.iter()
								.map(|k| self.blockchain.storage_at(block_number, k))
								.collect();
							let values = futures::future::join_all(futs).await;
							for (k, v) in keys.into_iter().zip(values) {
								let hash = match v {
									Ok(Some(val)) => Some(
										HexString::from_bytes(&sp_core::blake2_256(&val)).into(),
									),
									_ => None,
								};
								results.push(ArchiveStorageItem {
									key: HexString::from_bytes(&k).into(),
									value: None,
									hash,
								});
							}
						},
						Err(e) => {
							tracing::debug!(
								prefix = %item.key,
								error = %e,
								"archive_v1_storage: DescendantsHashes prefix lookup failed"
							);
						},
					}
					continue;
				},
				_ => {},
			}

			match self.blockchain.storage_at(block_number, &key_bytes).await {
				Ok(Some(value)) => match item.query_type {
					StorageQueryType::Value => {
						results.push(ArchiveStorageItem {
							key: item.key,
							value: Some(HexString::from_bytes(&value).into()),
							hash: None,
						});
					},
					StorageQueryType::Hash => {
						let hash = sp_core::blake2_256(&value);
						results.push(ArchiveStorageItem {
							key: item.key,
							value: None,
							hash: Some(HexString::from_bytes(&hash).into()),
						});
					},
					// Already handled above
					StorageQueryType::ClosestDescendantMerkleValue |
					StorageQueryType::DescendantsValues |
					StorageQueryType::DescendantsHashes => unreachable!(),
				},
				Ok(None) => {
					// Key doesn't exist - include in results with null value
					results.push(ArchiveStorageItem { key: item.key, value: None, hash: None });
				},
				Err(e) => {
					return Err(RpcServerError::Storage(e.to_string()).into());
				},
			}
		}
		Ok(ArchiveStorageResult::Ok { items: results })
	}

	async fn genesis_hash(&self) -> RpcResult<String> {
		self.blockchain.genesis_hash().await.map_err(|e| {
			RpcServerError::Internal(format!("Failed to fetch genesis hash: {e}")).into()
		})
	}

	async fn storage_diff(
		&self,
		hash: String,
		items: Vec<StorageDiffQueryItem>,
		previous_hash: Option<String>,
	) -> RpcResult<ArchiveStorageDiffResult> {
		let block_hash = parse_block_hash(&hash)?;

		// Get block number for the target block
		let block_number = match self.blockchain.block_number_by_hash(block_hash).await {
			Ok(Some(num)) => num,
			Ok(None) => {
				return Ok(ArchiveStorageDiffResult::Err { error: "Block not found".to_string() });
			},
			Err(e) => {
				return Err(RpcServerError::Internal(format!("Failed to resolve block: {e}")).into());
			},
		};

		// Determine the previous block hash
		let prev_block_hash = match previous_hash {
			Some(prev_hash_str) => parse_block_hash(&prev_hash_str)?,
			None => {
				// Get parent hash from the block
				match self.blockchain.block_parent_hash(block_hash).await {
					Ok(Some(parent_hash)) => parent_hash,
					Ok(None) => {
						return Ok(ArchiveStorageDiffResult::Err {
							error: "Block not found".to_string(),
						});
					},
					Err(e) => {
						return Err(RpcServerError::Internal(format!(
							"Failed to get parent hash: {e}"
						))
						.into());
					},
				}
			},
		};

		// Get block number for the previous block
		let prev_block_number = match self.blockchain.block_number_by_hash(prev_block_hash).await {
			Ok(Some(num)) => num,
			Ok(None) => {
				return Ok(ArchiveStorageDiffResult::Err {
					error: "Previous block not found".to_string(),
				});
			},
			Err(e) => {
				return Err(RpcServerError::Internal(format!(
					"Failed to resolve previous block: {e}"
				))
				.into());
			},
		};

		// Query storage for each item at both blocks and compute differences
		let mut results = Vec::new();
		for item in items {
			let key_bytes = parse_hex_bytes(&item.key, "key")?;

			// Get value at current block
			let current_value = match self.blockchain.storage_at(block_number, &key_bytes).await {
				Ok(v) => v,
				Err(e) => {
					return Err(RpcServerError::Storage(e.to_string()).into());
				},
			};

			// Get value at previous block
			let previous_value =
				match self.blockchain.storage_at(prev_block_number, &key_bytes).await {
					Ok(v) => v,
					Err(e) => {
						return Err(RpcServerError::Storage(e.to_string()).into());
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
							(Some(HexString::from_bytes(value).into()), None),
						StorageQueryType::Hash =>
							(None, Some(HexString::from_bytes(&sp_core::blake2_256(value)).into())),
						// Merkle/descendants types not applicable to diff - treat as value
						StorageQueryType::ClosestDescendantMerkleValue |
						StorageQueryType::DescendantsValues |
						StorageQueryType::DescendantsHashes => (Some(HexString::from_bytes(value).into()), None),
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
						StorageQueryType::Value => (Some(HexString::from_bytes(curr).into()), None),
						StorageQueryType::Hash =>
							(None, Some(HexString::from_bytes(&sp_core::blake2_256(curr)).into())),
						// Merkle/descendants types not applicable to diff - treat as value
						StorageQueryType::ClosestDescendantMerkleValue |
						StorageQueryType::DescendantsValues |
						StorageQueryType::DescendantsHashes => (Some(HexString::from_bytes(curr).into()), None),
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
mod tests {}
