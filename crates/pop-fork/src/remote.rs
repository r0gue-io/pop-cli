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
//! let storage = RemoteStorageLayer::new(rpc, cache, block_hash);
//!
//! // First call fetches from RPC and caches
//! let value = storage.get(&key).await?;
//!
//! // Second call returns cached value (no RPC call)
//! let value = storage.get(&key).await?;
//! ```

use crate::{ForkRpcClient, StorageCache, error::RemoteStorageError};
use subxt::config::substrate::H256;

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
#[derive(Clone)]
pub struct RemoteStorageLayer {
	rpc: ForkRpcClient,
	cache: StorageCache,
	block_hash: H256,
}

impl RemoteStorageLayer {
	/// Create a new remote storage layer.
	///
	/// # Arguments
	/// * `rpc` - RPC client connected to the live chain
	/// * `cache` - Storage cache for persisting fetched values
	/// * `block_hash` - Block hash to query state at (typically finalized head)
	pub fn new(rpc: ForkRpcClient, cache: StorageCache, block_hash: H256) -> Self {
		Self { rpc, cache, block_hash }
	}

	/// Get the block hash this layer is querying.
	pub fn block_hash(&self) -> H256 {
		self.block_hash
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
	pub async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, RemoteStorageError> {
		// Check cache first
		if let Some(cached) = self.cache.get_storage(self.block_hash, key).await? {
			return Ok(cached);
		}

		// Fetch from RPC
		let value = self.rpc.storage(key, self.block_hash).await?;

		// Cache the result (including empty values)
		self.cache.set_storage(self.block_hash, key, value.as_deref()).await?;

		Ok(value)
	}

	/// Get multiple storage values in a batch, fetching uncached keys from RPC.
	///
	/// # Arguments
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
		keys: &[&[u8]],
	) -> Result<Vec<Option<Vec<u8>>>, RemoteStorageError> {
		if keys.is_empty() {
			return Ok(vec![]);
		}

		// Check cache for all keys
		let cached_results = self.cache.get_storage_batch(self.block_hash, keys).await?;

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
		let fetched_values = self.rpc.storage_batch(&uncached_keys, self.block_hash).await?;

		// Cache fetched values
		let cache_entries: Vec<(&[u8], Option<&[u8]>)> = uncached_keys
			.iter()
			.zip(fetched_values.iter())
			.map(|(k, v)| (*k, v.as_deref()))
			.collect();

		if !cache_entries.is_empty() {
			self.cache.set_storage_batch(self.block_hash, &cache_entries).await?;
		}

		// Build final result, merging cached and fetched values
		let mut results: Vec<Option<Vec<u8>>> =
			cached_results.into_iter().map(|c| c.flatten()).collect();

		for (i, idx) in uncached_indices.into_iter().enumerate() {
			results[idx] = fetched_values[i].clone();
		}

		Ok(results)
	}

	/// Prefetch a range of storage keys by prefix.
	///
	/// Fetches all keys matching the prefix and caches their values.
	/// Useful for warming the cache before intensive operations.
	///
	/// # Arguments
	/// * `prefix` - Storage key prefix to match
	/// * `page_size` - Number of keys to fetch per RPC call
	///
	/// # Returns
	/// The total number of keys prefetched.
	pub async fn prefetch_prefix(
		&self,
		prefix: &[u8],
		page_size: u32,
	) -> Result<usize, RemoteStorageError> {
		let mut total_fetched = 0usize;
		let mut start_key: Option<Vec<u8>> = None;

		loop {
			// Get next page of keys
			let keys = self
				.rpc
				.storage_keys_paged(prefix, page_size, start_key.as_deref(), self.block_hash)
				.await?;

			if keys.is_empty() {
				break;
			}

			// Fetch values for these keys
			let key_refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
			let values = self.rpc.storage_batch(&key_refs, self.block_hash).await?;

			// Cache all key-value pairs
			let cache_entries: Vec<(&[u8], Option<&[u8]>)> =
				key_refs.iter().zip(values.iter()).map(|(k, v)| (*k, v.as_deref())).collect();

			self.cache.set_storage_batch(self.block_hash, &cache_entries).await?;

			total_fetched += keys.len();

			// Check if we've reached the end
			if keys.len() < page_size as usize {
				break;
			}

			// Set up for next page
			start_key = keys.into_iter().last();
		}

		Ok(total_fetched)
	}
}

// Unit tests for RemoteStorageLayer are limited without a live RPC endpoint.
// The cache behavior is thoroughly tested in cache.rs.
// Full integration tests covering the RPC -> cache flow are in tests/remote.rs
// with the `integration-tests` feature flag.
