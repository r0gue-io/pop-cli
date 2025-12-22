// SPDX-License-Identifier: GPL-3.0

//! Remote storage layer for lazy-loading state from live chains.
//!
//! This module provides the [`RemoteStorageLayer`] which transparently fetches storage
//! from a live chain via RPC when values aren't in the local cache. This enables
//! "lazy forking" where state is fetched on-demand rather than requiring a full sync.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    RemoteStorageLayer                            │
//! │                                                                   │
//! │   get(key) ─────► Cache Hit? ──── Yes ────► Return cached value │
//! │                        │                                         │
//! │                        No                                        │
//! │                        │                                         │
//! │                        ▼                                         │
//! │                 Fetch from RPC                                   │
//! │                        │                                         │
//! │                        ▼                                         │
//! │                 Store in cache                                   │
//! │                        │                                         │
//! │                        ▼                                         │
//! │                 Return value                                     │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::{ForkRpcClient, RemoteStorageLayer, StorageCache};
//!
//! let rpc = ForkRpcClient::connect(&"wss://rpc.polkadot.io".parse()?).await?;
//! let cache = StorageCache::in_memory().await?;
//! let block_hash = rpc.finalized_head().await?;
//!
//! let storage = RemoteStorageLayer::new(rpc, cache);
//!
//! // First call fetches from RPC and caches
//! let value = storage.get(block_hash, &key).await?;
//!
//! // Second call returns cached value (no RPC call)
//! let value = storage.get(block_hash, &key).await?;
//! ```

use crate::{ForkRpcClient, StorageCache, error::RemoteStorageError, models::BlockRow};
use subxt::{config::substrate::H256, ext::codec::Encode};

/// Default number of keys to fetch per RPC call during prefix scans.
///
/// This balances RPC overhead (fewer calls = better) against memory usage
/// and response latency. 1000 keys typically fits well within RPC response limits.
const DEFAULT_PREFETCH_PAGE_SIZE: u32 = 1000;

/// Remote storage layer that lazily fetches state from a live chain.
///
/// Provides a cache-through abstraction: reads check the local cache first,
/// and only fetch from the remote RPC when the value isn't cached. Fetched
/// values are automatically cached for subsequent reads.
///
/// # Cloning
///
/// `RemoteStorageLayer` is cheap to clone. Both `ForkRpcClient` and `StorageCache`
/// use internal reference counting (connection pools/Arc), so cloning just increments
/// reference counts.
///
/// # Thread Safety
///
/// The layer is `Send + Sync` and can be shared across async tasks. The underlying
/// cache handles concurrent access safely.
#[derive(Clone, Debug)]
pub struct RemoteStorageLayer {
	rpc: ForkRpcClient,
	cache: StorageCache,
}

impl RemoteStorageLayer {
	/// Create a new remote storage layer.
	///
	/// # Arguments
	/// * `rpc` - RPC client connected to the live chain
	/// * `cache` - Storage cache for persisting fetched values
	pub fn new(rpc: ForkRpcClient, cache: StorageCache) -> Self {
		Self { rpc, cache }
	}

	/// Get a reference to the underlying RPC client.
	pub fn rpc(&self) -> &ForkRpcClient {
		&self.rpc
	}

	/// Get a reference to the underlying cache.
	pub fn cache(&self) -> &StorageCache {
		&self.cache
	}

	/// Get a storage value, fetching from RPC if not cached.
	///
	/// # Returns
	/// * `Ok(Some(value))` - Storage exists with value
	/// * `Ok(None)` - Storage key doesn't exist (empty)
	/// * `Err(_)` - RPC or cache error
	///
	/// # Caching Behavior
	/// - If the key is in cache, returns the cached value immediately
	/// - If not cached, fetches from RPC, caches the result, and returns it
	/// - Empty storage (key exists but has no value) is cached as `None`
	pub async fn get(
		&self,
		block_hash: H256,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, RemoteStorageError> {
		// Check cache first
		if let Some(cached) = self.cache.get_storage(block_hash, key).await? {
			return Ok(cached);
		}

		// Fetch from RPC
		let value = self.rpc.storage(key, block_hash).await?;

		// Cache the result (including empty values)
		self.cache.set_storage(block_hash, key, value.as_deref()).await?;

		Ok(value)
	}

	/// Get multiple storage values in a batch, fetching uncached keys from RPC.
	///
	/// # Arguments
	/// * `block_hash` - The hash of the block being queried.
	/// * `keys` - Slice of storage keys to fetch (as byte slices to avoid unnecessary allocations)
	///
	/// # Returns
	/// A vector of optional values, in the same order as the input keys.
	///
	/// # Caching Behavior
	/// - Checks cache for all keys first
	/// - Only fetches uncached keys from RPC
	/// - Caches all fetched values (including empty ones)
	/// - Returns results in the same order as input keys
	pub async fn get_batch(
		&self,
		block_hash: H256,
		keys: &[&[u8]],
	) -> Result<Vec<Option<Vec<u8>>>, RemoteStorageError> {
		if keys.is_empty() {
			return Ok(vec![]);
		}

		// Check cache for all keys
		let cached_results = self.cache.get_storage_batch(block_hash, keys).await?;

		// Find which keys need to be fetched
		let mut uncached_indices: Vec<usize> = Vec::new();
		let mut uncached_keys: Vec<&[u8]> = Vec::new();

		for (i, cached) in cached_results.iter().enumerate() {
			if cached.is_none() {
				uncached_indices.push(i);
				uncached_keys.push(keys[i]);
			}
		}

		// If everything was cached, return immediately
		if uncached_keys.is_empty() {
			return Ok(cached_results.into_iter().map(|c| c.flatten()).collect());
		}

		// Fetch uncached keys from RPC
		let fetched_values = self.rpc.storage_batch(&uncached_keys, block_hash).await?;

		// Cache fetched values
		let cache_entries: Vec<(&[u8], Option<&[u8]>)> = uncached_keys
			.iter()
			.zip(fetched_values.iter())
			.map(|(k, v)| (*k, v.as_deref()))
			.collect();

		if !cache_entries.is_empty() {
			self.cache.set_storage_batch(block_hash, &cache_entries).await?;
		}

		// Build final result, merging cached and fetched values
		let mut results: Vec<Option<Vec<u8>>> =
			cached_results.into_iter().map(|c| c.flatten()).collect();

		for (i, idx) in uncached_indices.into_iter().enumerate() {
			results[idx] = fetched_values[i].clone();
		}

		Ok(results)
	}

	/// Prefetch a range of storage keys by prefix (resumable).
	///
	/// Fetches all keys matching the prefix and caches their values.
	/// This operation is resumable - if interrupted, calling it again will
	/// continue from where it left off.
	///
	/// # Arguments
	/// * `block_hash`.
	/// * `prefix` - Storage key prefix to match
	/// * `page_size` - Number of keys to fetch per RPC call
	///
	/// # Returns
	/// The total number of keys for this prefix (including previously cached).
	pub async fn prefetch_prefix(
		&self,
		block_hash: H256,
		prefix: &[u8],
		page_size: u32,
	) -> Result<usize, RemoteStorageError> {
		// Check existing progress
		let progress = self.cache.get_prefix_scan_progress(block_hash, prefix).await?;

		if let Some(ref p) = progress &&
			p.is_complete
		{
			// Already done - return cached count
			return Ok(self.cache.count_keys_by_prefix(block_hash, prefix).await?);
		}

		// Resume from last scanned key if we have progress
		let mut start_key = progress.and_then(|p| p.last_scanned_key);

		loop {
			// Get next page of keys
			let keys = self
				.rpc
				.storage_keys_paged(prefix, page_size, start_key.as_deref(), block_hash)
				.await?;

			if keys.is_empty() {
				// No keys found - mark as complete if this is the first page
				if start_key.is_none() {
					// Empty prefix, mark complete with empty marker
					self.cache.update_prefix_scan(block_hash, prefix, prefix, true).await?;
				}
				break;
			}

			// Determine pagination state before consuming keys
			let is_last_page = keys.len() < page_size as usize;

			// Fetch values for these keys
			let key_refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
			let values = self.rpc.storage_batch(&key_refs, block_hash).await?;

			// Cache all key-value pairs
			let cache_entries: Vec<(&[u8], Option<&[u8]>)> =
				key_refs.iter().zip(values.iter()).map(|(k, v)| (*k, v.as_deref())).collect();

			self.cache.set_storage_batch(block_hash, &cache_entries).await?;

			// Update progress with the last key from this page.
			// We consume keys here to avoid cloning for the next iteration's start_key.
			let last_key = keys.into_iter().last();
			if let Some(ref key) = last_key {
				self.cache.update_prefix_scan(block_hash, prefix, key, is_last_page).await?;
			}

			if is_last_page {
				break;
			}

			// Set up for next page (last_key is already owned, no extra allocation)
			start_key = last_key;
		}

		// Return total count (includes any previously cached keys)
		Ok(self.cache.count_keys_by_prefix(block_hash, prefix).await?)
	}

	/// Get all keys for a prefix, fetching from RPC if not fully cached.
	///
	/// This is a convenience method that:
	/// 1. Ensures the prefix is fully scanned (calls [`Self::prefetch_prefix`] if needed)
	/// 2. Returns all cached keys matching the prefix
	///
	/// Useful for enumerating all entries in a storage map (e.g., all accounts
	/// in a balances pallet).
	///
	/// # Arguments
	/// * `block_hash` - Block hash to query at
	/// * `prefix` - Storage key prefix to match (typically a pallet + storage item prefix)
	///
	/// # Returns
	/// All keys matching the prefix at the specified block hash.
	///
	/// # Performance
	/// First call may be slow if the prefix hasn't been scanned yet.
	/// Subsequent calls return cached data immediately.
	pub async fn get_keys(
		&self,
		block_hash: H256,
		prefix: &[u8],
	) -> Result<Vec<Vec<u8>>, RemoteStorageError> {
		// Ensure prefix is fully scanned
		self.prefetch_prefix(block_hash, prefix, DEFAULT_PREFETCH_PAGE_SIZE).await?;

		// Return from cache
		Ok(self.cache.get_keys_by_prefix(block_hash, prefix).await?)
	}

	/// Fetch a block by number from the remote RPC and cache it.
	///
	/// This method fetches the block data for the given block number and caches
	/// the block metadata in the cache.
	///
	/// # Arguments
	/// * `block_number` - The block number to fetch and cache
	///
	/// # Returns
	/// * `Ok(Some(block_row))` - Block was fetched and cached successfully
	/// * `Ok(None)` - Block number doesn't exist
	/// * `Err(_)` - RPC or cache error
	///
	/// # Caching Behavior
	/// - Fetches block hash and data from block number using `chain_getBlockHash` and
	///   `chain_getBlock`
	/// - Caches block metadata (hash, number, parent_hash, header) in the cache
	/// - If block is already cached, this will update the cache entry
	pub async fn fetch_and_cache_block_by_number(
		&self,
		block_number: u32,
	) -> Result<Option<BlockRow>, RemoteStorageError> {
		// Get block hash and full block data in one call
		let (block_hash, block) = match self.rpc.block_by_number(block_number).await? {
			Some((hash, block)) => (hash, block),
			None => return Ok(None),
		};

		// Extract header and parent hash
		let header = block.header;
		let parent_hash = header.parent_hash;
		let header_encoded = header.encode();

		// Cache the block
		self.cache
			.cache_block(block_hash, block_number, parent_hash, &header_encoded)
			.await?;

		// Return the cached block row
		Ok(Some(BlockRow {
			hash: block_hash.as_bytes().to_vec(),
			number: block_number as i64,
			parent_hash: parent_hash.as_bytes().to_vec(),
			header: header_encoded,
		}))
	}

	/// Get the next key after the given key that starts with the prefix.
	///
	/// This method is used for key enumeration during runtime execution.
	/// It fetches keys directly from the RPC without caching intermediate results.
	///
	/// # Arguments
	/// * `prefix` - Storage key prefix to match
	/// * `key` - The current key; returns the next key after this one
	///
	/// # Returns
	/// * `Ok(Some(key))` - The next key after `key` that starts with `prefix`
	/// * `Ok(None)` - No more keys with this prefix
	pub async fn next_key(
		&self,
		prefix: &[u8],
		key: &[u8],
	) -> Result<Option<Vec<u8>>, RemoteStorageError> {
		// Fetch just 1 key after the current key
		let keys = self.rpc.storage_keys_paged(prefix, 1, Some(key), self.block_hash).await?;
		Ok(keys.into_iter().next())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn error_display_rpc() {
		use crate::error::RpcClientError;
		let inner = RpcClientError::InvalidResponse("test".to_string());
		let err = RemoteStorageError::Rpc(inner);
		assert!(err.to_string().contains("RPC error"));
	}

	#[test]
	fn error_display_cache() {
		use crate::error::CacheError;
		let inner = CacheError::DataCorruption("test".to_string());
		let err = RemoteStorageError::Cache(inner);
		assert!(err.to_string().contains("Cache error"));
	}

	/// Integration tests that spawn local test nodes.
	///
	/// These tests are run sequentially via nextest configuration to avoid
	/// concurrent node downloads causing race conditions.
	mod sequential {
		use super::*;
		use pop_common::test_env::TestNode;
		use std::time::Duration;
		use url::Url;

		// Well-known storage keys for testing.
		// These are derived from twox128 hashes of pallet and storage item names.

		/// System::Number storage key: twox128("System") ++ twox128("Number")
		const SYSTEM_NUMBER_KEY: &str =
			"26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac";

		/// System::ParentHash storage key: twox128("System") ++ twox128("ParentHash")
		const SYSTEM_PARENT_HASH_KEY: &str =
			"26aa394eea5630e07c48ae0c9558cef734abf5cb34d6244378cddbf18e849d96";

		/// System pallet prefix: twox128("System")
		const SYSTEM_PALLET_PREFIX: &str = "26aa394eea5630e07c48ae0c9558cef7";

		/// Helper struct to hold the test node and layer together.
		/// This ensures the node stays alive for the duration of the test.
		struct TestContext {
			#[allow(dead_code)]
			node: TestNode,
			layer: RemoteStorageLayer,
			block_hash: H256,
		}

		async fn create_test_context() -> TestContext {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let rpc = ForkRpcClient::connect(&endpoint).await.unwrap();
			let cache = StorageCache::in_memory().await.unwrap();
			let block_hash = rpc.finalized_head().await.unwrap();
			let layer = RemoteStorageLayer::new(rpc, cache);

			TestContext { node, layer, block_hash }
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn get_fetches_and_caches() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;
			let block_hash = ctx.block_hash;

			let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

			// First call should fetch from RPC and cache
			let value1 = layer.get(block_hash, &key).await.unwrap();
			assert!(value1.is_some(), "System::Number should exist");

			// Verify it was cached
			let cached = layer.cache().get_storage(block_hash, &key).await.unwrap();
			assert!(cached.is_some(), "Value should be cached after first get");
			assert_eq!(cached.unwrap(), value1);

			// Second call should return cached value (same result)
			let value2 = layer.get(block_hash, &key).await.unwrap();
			assert_eq!(value1, value2);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn get_caches_empty_values() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;
			let block_hash = ctx.block_hash;

			// Use a key that definitely doesn't exist
			let nonexistent_key = b"this_key_definitely_does_not_exist_12345";

			// First call fetches from RPC - should be None
			let value = layer.get(block_hash, nonexistent_key).await.unwrap();
			assert!(value.is_none(), "Nonexistent key should return None");

			// Verify it was cached as empty (Some(None))
			let cached = layer.cache().get_storage(block_hash, nonexistent_key).await.unwrap();
			assert_eq!(cached, Some(None), "Empty value should be cached as Some(None)");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn get_batch_fetches_mixed() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;
			let block_hash = ctx.block_hash;

			let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
			let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
			let key3 = b"nonexistent_key".to_vec();

			let keys: Vec<&[u8]> = vec![key1.as_slice(), key2.as_slice(), key3.as_slice()];

			let results = layer.get_batch(block_hash, &keys).await.unwrap();

			assert_eq!(results.len(), 3);
			assert!(results[0].is_some(), "System::Number should exist");
			assert!(results[1].is_some(), "System::ParentHash should exist");
			assert!(results[2].is_none(), "Nonexistent key should be None");

			// Verify all were cached
			for (i, key) in keys.iter().enumerate() {
				let cached = layer.cache().get_storage(block_hash, key).await.unwrap();
				assert!(cached.is_some(), "Key {} should be cached", i);
			}
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn get_batch_uses_cache() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;
			let block_hash = ctx.block_hash;

			let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
			let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

			// Pre-cache key1
			let value1 = layer.get(block_hash, &key1).await.unwrap();

			// Batch get with one cached and one uncached
			let keys: Vec<&[u8]> = vec![key1.as_slice(), key2.as_slice()];
			let results = layer.get_batch(block_hash, &keys).await.unwrap();

			assert_eq!(results.len(), 2);
			assert_eq!(results[0], value1, "Cached value should match");
			assert!(results[1].is_some(), "Uncached value should be fetched");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn prefetch_prefix() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;
			let block_hash = ctx.block_hash;

			let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

			// Prefetch all System storage items (page_size is the batch size per RPC call)
			let count = layer.prefetch_prefix(block_hash, &prefix, 5).await.unwrap();

			assert!(count > 0, "Should have prefetched some keys");

			// Verify some values were cached
			let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
			let cached = layer.cache().get_storage(block_hash, &key).await.unwrap();
			assert!(cached.is_some(), "Prefetched key should be cached");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn layer_is_cloneable() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;
			let block_hash = ctx.block_hash;

			// Clone the layer
			let layer2 = layer.clone();

			// Both should work and share the same cache
			let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

			let value1 = layer.get(block_hash, &key).await.unwrap();
			let value2 = layer2.get(block_hash, &key).await.unwrap();

			assert_eq!(value1, value2);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn accessor_methods() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;
			let block_hash = ctx.block_hash;

			// Test accessor methods
			assert!(!block_hash.is_zero());
			// Verify endpoint is a valid WebSocket URL (from our local test node)
			assert!(layer.rpc().endpoint().as_str().starts_with("ws://"));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fetch_and_cache_block_by_number_caches_block() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;

			// Get finalized block number
			let finalized_hash = layer.rpc().finalized_head().await.unwrap();
			let finalized_header = layer.rpc().header(finalized_hash).await.unwrap();
			let finalized_number = finalized_header.number;

			// Verify block is not in cache initially
			let cached = layer.cache().get_block_by_number(finalized_number).await.unwrap();

			assert!(cached.is_none());

			// Fetch and cache the block
			let result = layer.fetch_and_cache_block_by_number(finalized_number).await.unwrap();
			assert!(result.is_some());

			let block_row = result.unwrap();
			assert_eq!(block_row.number, finalized_number as i64);
			assert_eq!(block_row.hash.len(), 32);
			assert_eq!(block_row.parent_hash.len(), 32);
			assert!(!block_row.header.is_empty());

			// Verify it's now in cache
			let cached = layer.cache().get_block_by_number(finalized_number).await.unwrap();
			assert!(cached.is_some());

			let cached_block = cached.unwrap();
			assert_eq!(cached_block.number, block_row.number);
			assert_eq!(cached_block.hash, block_row.hash);
			assert_eq!(cached_block.parent_hash, block_row.parent_hash);
			assert_eq!(cached_block.header, block_row.header);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fetch_and_cache_block_by_number_non_existent() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;

			// Try to fetch a block that doesn't exist
			let non_existent_number = u32::MAX;
			let result = layer.fetch_and_cache_block_by_number(non_existent_number).await.unwrap();

			assert!(result.is_none(), "Non-existent block should return None");

			// Verify it's not in cache
			let cached = layer.cache().get_block_by_number(non_existent_number).await.unwrap();
			assert!(cached.is_none(), "Non-existent block should not be cached");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fetch_and_cache_block_by_number_multiple_blocks() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;

			// Wait for some blocks to be finalized
			std::thread::sleep(Duration::from_secs(30));

			// Get finalized block number
			let finalized_hash = layer.rpc().finalized_head().await.unwrap();
			let finalized_header = layer.rpc().header(finalized_hash).await.unwrap();
			let finalized_number = finalized_header.number;

			// Fetch and cache multiple blocks
			let max_blocks = finalized_number.min(3);
			for block_num in 0..=max_blocks {
				let result =
					layer.fetch_and_cache_block_by_number(block_num).await.unwrap().unwrap();

				assert_eq!(result.number, block_num as i64);

				// Verify in cache
				let cached = layer.cache().get_block_by_number(block_num).await.unwrap().unwrap();
				assert_eq!(cached.number, result.number);
				assert_eq!(cached.hash, result.hash);
			}
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fetch_and_cache_block_by_number_idempotent() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;

			let block_number = 0u32;

			// Fetch and cache the block twice
			let result1 =
				layer.fetch_and_cache_block_by_number(block_number).await.unwrap().unwrap();
			let result2 =
				layer.fetch_and_cache_block_by_number(block_number).await.unwrap().unwrap();

			// Both results should be identical
			assert_eq!(result1.number, result2.number);
			assert_eq!(result1.hash, result2.hash);
			assert_eq!(result1.parent_hash, result2.parent_hash);
			assert_eq!(result1.header, result2.header);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fetch_and_cache_block_by_number_verifies_parent_chain() {
			let ctx = create_test_context().await;
			let layer = &ctx.layer;

			// Wait for some blocks to be finalized
			std::thread::sleep(Duration::from_secs(30));

			// Get finalized block number
			let finalized_hash = layer.rpc().finalized_head().await.unwrap();
			let finalized_header = layer.rpc().header(finalized_hash).await.unwrap();
			let finalized_number = finalized_header.number;

			// Fetch consecutive blocks and verify parent hash chain
			let max_blocks = finalized_number.min(3);
			let mut previous_hash: Option<Vec<u8>> = None;

			for block_num in 0..=max_blocks {
				let block_row =
					layer.fetch_and_cache_block_by_number(block_num).await.unwrap().unwrap();

				// Verify parent hash matches previous block hash (except for genesis)
				if let Some(prev_hash) = previous_hash {
					assert_eq!(
						block_row.parent_hash,
						prev_hash,
						"Block {} parent hash should match block {} hash",
						block_num,
						block_num - 1
					);
				}

				previous_hash = Some(block_row.hash.clone());
			}
		}
	}
}
