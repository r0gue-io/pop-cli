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

use crate::{
	ForkRpcClient, StorageCache,
	error::{RemoteStorageError, RpcClientError},
	models::BlockRow,
};
use std::sync::{
	Arc,
	atomic::{AtomicUsize, Ordering},
};
use subxt::{Metadata, config::substrate::H256, ext::codec::Encode};

/// Default number of keys to fetch per RPC call during prefix scans.
///
/// This balances RPC overhead (fewer calls = better) against memory usage
/// and response latency. 1000 keys typically fits well within RPC response limits.
const DEFAULT_PREFETCH_PAGE_SIZE: u32 = 1000;

/// Minimum key length (bytes) for speculative prefix prefetch.
///
/// Polkadot SDK storage keys are composed of twox128(pallet) + twox128(item) = 32 bytes.
/// Keys shorter than this are pallet-level prefixes rather than storage item keys,
/// so speculative prefix scans on them would be too broad.
const MIN_STORAGE_KEY_PREFIX_LEN: usize = 32;

/// Counters tracking cache hits vs RPC misses for performance analysis.
///
/// All counters are atomic and shared across clones of the same `RemoteStorageLayer`.
/// Use [`RemoteStorageLayer::reset_stats`] to zero them before a phase, and
/// [`RemoteStorageLayer::stats`] to read the snapshot.
#[derive(Debug, Default)]
pub struct StorageStats {
	/// Number of `get()` calls served from cache (no RPC).
	pub cache_hits: AtomicUsize,
	/// Number of `get()` calls that triggered a speculative prefetch and the
	/// prefetch covered the requested key (cache hit after prefetch).
	pub prefetch_hits: AtomicUsize,
	/// Number of `get()` calls that fell through to an individual `state_getStorage` RPC.
	pub rpc_misses: AtomicUsize,
	/// Number of `next_key()` calls served from cache.
	pub next_key_cache: AtomicUsize,
	/// Number of `next_key()` calls that hit RPC.
	pub next_key_rpc: AtomicUsize,
}

/// Snapshot of [`StorageStats`] counters at a point in time.
#[derive(Debug, Clone, Default)]
pub struct StorageStatsSnapshot {
	pub cache_hits: usize,
	pub prefetch_hits: usize,
	pub rpc_misses: usize,
	pub next_key_cache: usize,
	pub next_key_rpc: usize,
}

impl std::fmt::Display for StorageStatsSnapshot {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let total_get = self.cache_hits + self.prefetch_hits + self.rpc_misses;
		let total_next = self.next_key_cache + self.next_key_rpc;
		write!(
			f,
			"get: {} total ({} cache, {} prefetch, {} rpc) | next_key: {} total ({} cache, {} rpc)",
			total_get,
			self.cache_hits,
			self.prefetch_hits,
			self.rpc_misses,
			total_next,
			self.next_key_cache,
			self.next_key_rpc,
		)
	}
}

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
	stats: Arc<StorageStats>,
}

impl RemoteStorageLayer {
	/// Create a new remote storage layer.
	///
	/// # Arguments
	/// * `rpc` - RPC client connected to the live chain
	/// * `cache` - Storage cache for persisting fetched values
	pub fn new(rpc: ForkRpcClient, cache: StorageCache) -> Self {
		Self { rpc, cache, stats: Arc::new(StorageStats::default()) }
	}

	/// Get a reference to the underlying RPC client.
	pub fn rpc(&self) -> &ForkRpcClient {
		&self.rpc
	}

	/// Get a reference to the underlying cache.
	pub fn cache(&self) -> &StorageCache {
		&self.cache
	}

	/// Get the RPC endpoint URL this layer is connected to.
	pub fn endpoint(&self) -> &url::Url {
		self.rpc.endpoint()
	}

	/// Take a snapshot of the current storage access counters.
	pub fn stats(&self) -> StorageStatsSnapshot {
		StorageStatsSnapshot {
			cache_hits: self.stats.cache_hits.load(Ordering::Relaxed),
			prefetch_hits: self.stats.prefetch_hits.load(Ordering::Relaxed),
			rpc_misses: self.stats.rpc_misses.load(Ordering::Relaxed),
			next_key_cache: self.stats.next_key_cache.load(Ordering::Relaxed),
			next_key_rpc: self.stats.next_key_rpc.load(Ordering::Relaxed),
		}
	}

	/// Reset all storage access counters to zero.
	pub fn reset_stats(&self) {
		self.stats.cache_hits.store(0, Ordering::Relaxed);
		self.stats.prefetch_hits.store(0, Ordering::Relaxed);
		self.stats.rpc_misses.store(0, Ordering::Relaxed);
		self.stats.next_key_cache.store(0, Ordering::Relaxed);
		self.stats.next_key_rpc.store(0, Ordering::Relaxed);
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
	/// - If not cached and the key is >= 32 bytes, speculatively prefetches the first page of keys
	///   sharing the same 32-byte prefix (pallet hash + storage item hash). This converts hundreds
	///   of individual RPCs into a handful of bulk fetches without risking a full scan of large
	///   maps.
	/// - Falls back to individual RPC fetch if the key is short or the speculative prefetch didn't
	///   cover it (key beyond first page).
	/// - Empty storage (key exists but has no value) is cached as `None`
	pub async fn get(
		&self,
		block_hash: H256,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, RemoteStorageError> {
		// Check cache first
		if let Some(cached) = self.cache.get_storage(block_hash, key).await? {
			self.stats.cache_hits.fetch_add(1, Ordering::Relaxed);
			return Ok(cached);
		}

		// Speculative prefix prefetch: if the key is at least 32 bytes (pallet hash +
		// storage item hash), bulk-fetch the FIRST PAGE of keys sharing that prefix.
		// Only fetches one page to avoid blocking on large maps (e.g., Account maps
		// with thousands of entries). This still captures the majority of runtime
		// reads since most storage items have fewer than 1000 keys.
		//
		// Errors are non-fatal: speculative prefetch is an optimization. If the
		// connection drops mid-prefetch, we fall through to the individual fetch
		// below which has its own retry logic.
		if key.len() >= MIN_STORAGE_KEY_PREFIX_LEN {
			let prefix = &key[..MIN_STORAGE_KEY_PREFIX_LEN];
			let progress = self.cache.get_prefix_scan_progress(block_hash, prefix).await?;
			if progress.is_none() {
				match self
					.prefetch_prefix_single_page(block_hash, prefix, DEFAULT_PREFETCH_PAGE_SIZE)
					.await
				{
					Ok(_) => {
						// Check cache again, the prefetch likely fetched our key
						if let Some(cached) = self.cache.get_storage(block_hash, key).await? {
							self.stats.prefetch_hits.fetch_add(1, Ordering::Relaxed);
							return Ok(cached);
						}
					},
					Err(e) => {
						log::debug!(
							"Speculative prefetch failed (non-fatal), falling through to individual fetch: {e}"
						);
					},
				}
			}
		}

		// Fallback: fetch individual key from RPC (with reconnect-retry)
		self.stats.rpc_misses.fetch_add(1, Ordering::Relaxed);
		let value = match self.rpc.storage(key, block_hash).await {
			Ok(v) => v,
			Err(_) => {
				self.rpc.reconnect().await?;
				self.rpc.storage(key, block_hash).await?
			},
		};

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

		// Fetch uncached keys from RPC (with reconnect-retry)
		let fetched_values = match self.rpc.storage_batch(&uncached_keys, block_hash).await {
			Ok(v) => v,
			Err(_) => {
				self.rpc.reconnect().await?;
				self.rpc.storage_batch(&uncached_keys, block_hash).await?
			},
		};

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
			// Get next page of keys (with reconnect-retry)
			let keys = match self
				.rpc
				.storage_keys_paged(prefix, page_size, start_key.as_deref(), block_hash)
				.await
			{
				Ok(v) => v,
				Err(_) => {
					self.rpc.reconnect().await?;
					self.rpc
						.storage_keys_paged(prefix, page_size, start_key.as_deref(), block_hash)
						.await?
				},
			};

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

			// Fetch values for these keys (with reconnect-retry)
			let key_refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
			let values = match self.rpc.storage_batch(&key_refs, block_hash).await {
				Ok(v) => v,
				Err(_) => {
					self.rpc.reconnect().await?;
					self.rpc.storage_batch(&key_refs, block_hash).await?
				},
			};

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

	/// Fetch a single page of keys for a prefix and cache their values.
	///
	/// Unlike [`prefetch_prefix`](Self::prefetch_prefix), this fetches only the first
	/// page of keys (up to `page_size`) without looping through subsequent pages.
	/// This keeps the cost bounded regardless of how many keys exist under the prefix.
	///
	/// Records scan progress so that subsequent calls to `prefetch_prefix` can
	/// resume from where this left off.
	pub async fn prefetch_prefix_single_page(
		&self,
		block_hash: H256,
		prefix: &[u8],
		page_size: u32,
	) -> Result<usize, RemoteStorageError> {
		// Check existing progress
		let progress = self.cache.get_prefix_scan_progress(block_hash, prefix).await?;

		if let Some(ref p) = progress {
			if p.is_complete {
				return Ok(self.cache.count_keys_by_prefix(block_hash, prefix).await?);
			}
			// A scan is already in progress (from a concurrent call or prior run),
			// don't start another one.
			return Ok(0);
		}

		// Fetch first page of keys (with reconnect-retry)
		let keys = match self.rpc.storage_keys_paged(prefix, page_size, None, block_hash).await {
			Ok(v) => v,
			Err(_) => {
				self.rpc.reconnect().await?;
				self.rpc.storage_keys_paged(prefix, page_size, None, block_hash).await?
			},
		};

		if keys.is_empty() {
			self.cache.update_prefix_scan(block_hash, prefix, prefix, true).await?;
			return Ok(0);
		}

		let is_last_page = keys.len() < page_size as usize;

		// Fetch values for these keys (with reconnect-retry)
		let key_refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
		let values = match self.rpc.storage_batch(&key_refs, block_hash).await {
			Ok(v) => v,
			Err(_) => {
				self.rpc.reconnect().await?;
				self.rpc.storage_batch(&key_refs, block_hash).await?
			},
		};

		// Cache all key-value pairs
		let cache_entries: Vec<(&[u8], Option<&[u8]>)> =
			key_refs.iter().zip(values.iter()).map(|(k, v)| (*k, v.as_deref())).collect();

		self.cache.set_storage_batch(block_hash, &cache_entries).await?;

		let count = keys.len();
		if let Some(last_key) = keys.into_iter().last() {
			self.cache
				.update_prefix_scan(block_hash, prefix, &last_key, is_last_page)
				.await?;
		}

		Ok(count)
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
	/// Before hitting the RPC, it checks whether a complete prefix scan exists
	/// in the cache for the queried prefix (or parent prefixes at 32 or 16 bytes).
	/// If so, the answer is served from the local SQLite cache, avoiding an RPC
	/// round-trip entirely.
	///
	/// # Arguments
	/// * `block_hash` - Block hash to query at
	/// * `prefix` - Storage key prefix to match
	/// * `key` - The current key; returns the next key after this one
	///
	/// # Returns
	/// * `Ok(Some(key))` - The next key after `key` that starts with `prefix`
	/// * `Ok(None)` - No more keys with this prefix
	pub async fn next_key(
		&self,
		block_hash: H256,
		prefix: &[u8],
		key: &[u8],
	) -> Result<Option<Vec<u8>>, RemoteStorageError> {
		// Check if we have a complete prefix scan that covers this query.
		// Try the exact prefix first, then common parent lengths (32-byte = pallet+item,
		// 16-byte = pallet-only).
		let candidate_lengths: &[usize] = &[prefix.len(), 32, 16];
		for &len in candidate_lengths {
			if len > prefix.len() {
				continue;
			}
			let candidate = &prefix[..len];
			if let Some(progress) =
				self.cache.get_prefix_scan_progress(block_hash, candidate).await? &&
				progress.is_complete
			{
				self.stats.next_key_cache.fetch_add(1, Ordering::Relaxed);
				return Ok(self.cache.next_key_from_cache(block_hash, prefix, key).await?);
			}
		}

		// Fallback: fetch from RPC (with reconnect-retry)
		self.stats.next_key_rpc.fetch_add(1, Ordering::Relaxed);
		let keys = match self.rpc.storage_keys_paged(prefix, 1, Some(key), block_hash).await {
			Ok(v) => v,
			Err(_) => {
				self.rpc.reconnect().await?;
				self.rpc.storage_keys_paged(prefix, 1, Some(key), block_hash).await?
			},
		};
		Ok(keys.into_iter().next())
	}

	// ============================================================================
	// Block and header fetching methods
	// ============================================================================
	// These methods provide access to block data from the remote chain,
	// allowing Blockchain to delegate remote queries without directly
	// interfacing with ForkRpcClient.

	/// Get block body (extrinsics) by hash from the remote chain.
	///
	/// # Returns
	/// * `Ok(Some(extrinsics))` - Block found, returns list of encoded extrinsics
	/// * `Ok(None)` - Block not found
	pub async fn block_body(&self, hash: H256) -> Result<Option<Vec<Vec<u8>>>, RemoteStorageError> {
		match self.rpc.block_by_hash(hash).await? {
			Some(block) => {
				let extrinsics = block.extrinsics.into_iter().map(|ext| ext.0.to_vec()).collect();
				Ok(Some(extrinsics))
			},
			None => Ok(None),
		}
	}

	/// Get block header by hash from the remote chain.
	///
	/// # Returns
	/// * `Ok(Some(header_bytes))` - Encoded header bytes
	/// * `Ok(None)` - Block not found on the remote chain
	/// * `Err(..)` - Transport/connection error (caller should retry or reconnect)
	pub async fn block_header(&self, hash: H256) -> Result<Option<Vec<u8>>, RemoteStorageError> {
		match self.rpc.header(hash).await {
			Ok(header) => Ok(Some(header.encode())),
			// Header not found (RPC returned null): legitimate "not found"
			Err(RpcClientError::InvalidResponse(_)) => Ok(None),
			// Connection/transport errors must be propagated so callers can reconnect
			Err(e) => Err(e.into()),
		}
	}

	/// Get block hash by block number from the remote chain.
	///
	/// # Returns
	/// * `Ok(Some(hash))` - Block hash at the given number
	/// * `Ok(None)` - Block number not found
	pub async fn block_hash_by_number(
		&self,
		block_number: u32,
	) -> Result<Option<H256>, RemoteStorageError> {
		Ok(self.rpc.block_hash_at(block_number).await?)
	}

	/// Get block number by hash from the remote chain.
	///
	/// This method checks the persistent SQLite cache first before hitting RPC.
	/// Results are cached for future lookups.
	///
	/// # Returns
	/// * `Ok(Some(number))` - Block number for the given hash
	/// * `Ok(None)` - Block not found
	pub async fn block_number_by_hash(
		&self,
		hash: H256,
	) -> Result<Option<u32>, RemoteStorageError> {
		// Check cache first
		if let Some(block) = self.cache.get_block(hash).await? {
			return Ok(Some(block.number as u32));
		}

		// Fetch from RPC
		match self.rpc.block_by_hash(hash).await? {
			Some(block) => {
				let number = block.header.number;
				let parent_hash = block.header.parent_hash;
				let header_encoded = block.header.encode();

				// Cache for future lookups
				self.cache.cache_block(hash, number, parent_hash, &header_encoded).await?;

				Ok(Some(number))
			},
			None => Ok(None),
		}
	}

	/// Get parent hash of a block from the remote chain.
	///
	/// This method checks the persistent SQLite cache first before hitting RPC.
	/// Results are cached for future lookups.
	///
	/// # Returns
	/// * `Ok(Some(parent_hash))` - Parent hash of the block
	/// * `Ok(None)` - Block not found
	pub async fn parent_hash(&self, hash: H256) -> Result<Option<H256>, RemoteStorageError> {
		// Check cache first
		if let Some(block) = self.cache.get_block(hash).await? {
			let parent_hash = H256::from_slice(&block.parent_hash);
			return Ok(Some(parent_hash));
		}

		// Fetch from RPC
		match self.rpc.block_by_hash(hash).await? {
			Some(block) => {
				let number = block.header.number;
				let parent_hash = block.header.parent_hash;
				let header_encoded = block.header.encode();

				// Cache for future lookups
				self.cache.cache_block(hash, number, parent_hash, &header_encoded).await?;

				Ok(Some(parent_hash))
			},
			None => Ok(None),
		}
	}

	/// Get full block data (hash and block) by number from the remote chain.
	///
	/// # Returns
	/// * `Ok(Some((hash, block)))` - Block found
	/// * `Ok(None)` - Block number not found
	pub async fn block_by_number(
		&self,
		block_number: u32,
	) -> Result<
		Option<(H256, subxt::backend::legacy::rpc_methods::Block<subxt::SubstrateConfig>)>,
		RemoteStorageError,
	> {
		Ok(self.rpc.block_by_number(block_number).await?)
	}

	/// Get the latest finalized block hash from the remote chain.
	pub async fn finalized_head(&self) -> Result<H256, RemoteStorageError> {
		Ok(self.rpc.finalized_head().await?)
	}

	/// Get decoded metadata at a specific block from the remote chain.
	pub async fn metadata(&self, block_hash: H256) -> Result<Metadata, RemoteStorageError> {
		Ok(self.rpc.metadata(block_hash).await?)
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
}
