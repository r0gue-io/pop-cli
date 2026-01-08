// SPDX-License-Identifier: GPL-3.0

//! Local storage layer for tracking modifications to forked state.
//!
//! This module provides the [`LocalStorageLayer`] which sits on top of a [`RemoteStorageLayer`]
//! and tracks local modifications without mutating the underlying cached state. This enables
//! transactional semantics where changes can be committed, discarded, or merged.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    LocalStorageLayer                             │
//! │                                                                   │
//! │   get(key) ─────► Modified? ──── Yes ────► Return modified value│
//! │                        │                                          │
//! │                        No                                         │
//! │                        │                                          │
//! │                        ▼                                          │
//! │                 Prefix deleted? ── Yes ───► Return None          │
//! │                        │                                          │
//! │                        No                                         │
//! │                        │                                          │
//! │                        ▼                                          │
//! │                 Query parent layer                                │
//! │                        │                                          │
//! │                        ▼                                          │
//! │                 Return value                                      │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::{LocalStorageLayer, RemoteStorageLayer};
//!
//! let remote = RemoteStorageLayer::new(rpc, cache, block_hash);
//! let local = LocalStorageLayer::new(remote);
//!
//! // Set a value locally (doesn't affect remote/cache)
//! local.set(&key, Some(&value))?;
//!
//! // Read returns the modified value
//! let value = local.get(&key).await?;
//!
//! // Delete all keys with a prefix
//! local.delete_prefix(&prefix)?;
//! ```

use crate::{error::LocalStorageError, models::BlockRow, remote::RemoteStorageLayer};
use std::{
	collections::HashMap,
	sync::{Arc, RwLock},
};
use subxt::config::substrate::H256;

type SharedValue = Arc<Vec<u8>>;
type Modifications = HashMap<Vec<u8>, Option<SharedValue>>;
type DeletedPrefixes = Vec<Vec<u8>>;
type DiffLocalStorage = Vec<(Vec<u8>, Option<SharedValue>)>;

/// Local storage layer that tracks modifications on top of a remote layer.
///
/// Provides transactional semantics: modifications are tracked locally without
/// affecting the underlying remote layer or cache. Changes can be inspected via
/// [`diff`](Self::diff), merged with [`merge`](Self::merge), or child layers
/// can be created with [`child`](Self::child).
///
/// # Block-based Storage Strategy
///
/// - `latest_block_number`: Current working block number (modifications in HashMap)
/// - `first_forked_block_number`: Initial fork point (immutable)
/// - Blocks between first_forked_block_number and latest_block_number are in local_storage table
/// - Blocks before first_forked_block_number come from remote provider
///
/// # Cloning
///
/// `LocalStorageLayer` is cheap to clone. The underlying modifications and
/// deleted prefixes use `Arc<RwLock<_>>`, so clones share the same state.
///
/// # Thread Safety
///
/// The layer is `Send + Sync` and can be shared across async tasks. All
/// operations use `read`/`write` locks which will block until the lock is acquired.
#[derive(Clone, Debug)]
pub struct LocalStorageLayer {
	parent: RemoteStorageLayer,
	first_forked_block_hash: H256,
	first_forked_block_number: u32,
	latest_block_number: u32,
	modifications: Arc<RwLock<Modifications>>,
	deleted_prefixes: Arc<RwLock<DeletedPrefixes>>,
}

impl LocalStorageLayer {
	/// Create a new local storage layer.
	///
	/// # Arguments
	/// * `parent` - The remote storage layer to use as the base state
	/// * `first_forked_block_number` - The initial block number where the fork started
	///
	/// # Returns
	/// A new `LocalStorageLayer` with no modifications, with `latest_block_number` set to
	/// `first_forked_block_number`.
	pub fn new(
		parent: RemoteStorageLayer,
		first_forked_block_number: u32,
		first_forked_block_hash: H256,
	) -> Self {
		Self {
			parent,
			first_forked_block_hash,
			first_forked_block_number,
			latest_block_number: first_forked_block_number,
			modifications: Arc::new(RwLock::new(HashMap::new())),
			deleted_prefixes: Arc::new(RwLock::new(Vec::new())),
		}
	}

	/// Fetch and cache a block if it's not already in the cache.
	///
	/// This is a helper method used by `get` and `get_batch` to ensure blocks are
	/// available in the cache before querying them.
	///
	/// # Arguments
	/// * `block_number` - The block number to fetch and cache
	///
	/// # Returns
	/// * `Ok(Some(block_row))` - Block is now in cache (either was already cached or just fetched)
	/// * `Ok(None)` - Block number doesn't exist
	/// * `Err(_)` - RPC or cache error
	///
	/// # Behavior
	/// - First checks if block is already in cache
	/// - If not cached, fetches from remote RPC and caches it
	/// - If block doesn't exist, returns None
	async fn fetch_and_cache_block_if_needed(
		&self,
		block_number: u32,
	) -> Result<Option<BlockRow>, LocalStorageError> {
		// First check if block is already in cache
		if let Some(cached_block) = self.parent.cache().get_block_by_number(block_number).await? {
			return Ok(Some(cached_block));
		}

		// Not in cache, fetch from remote and cache it
		Ok(self.parent.fetch_and_cache_block_by_number(block_number).await?)
	}

	/// Get the current latest block number.
	pub fn get_latest_block_number(&self) -> u32 {
		self.latest_block_number
	}

	/// Get a storage value, checking local modifications first.
	///
	/// # Arguments
	/// * `block_number` - The block number to query
	/// * `key` - The storage key to fetch
	///
	/// # Returns
	/// * `Ok(Some(value))` - Value exists (either modified locally, in local_storage, or in parent)
	/// * `Ok(None)` - Key doesn't exist or was deleted via prefix deletion
	/// * `Err(_)` - Lock error or parent layer error
	///
	/// # Behavior
	/// Storage lookup strategy based on block_number:
	/// 1. If `block_number == latest_block_number`: Check modifications HashMap, then remote at
	///    first_forked_block
	/// 2. If `first_forked_block_number < block_number < latest_block_number`: Check local_storage
	///    table
	/// 3. Otherwise: Check remote provider directly (fetches block_hash from blocks table)
	pub async fn get(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<SharedValue>, LocalStorageError> {
		let latest_block_number = self.get_latest_block_number();

		// Case 1: Query for latest block - check modifications, then remote at first_forked_block
		if block_number == latest_block_number {
			{
				let modifications_lock = self
					.modifications
					.read()
					.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
				let deleted_prefixes_lock = self
					.deleted_prefixes
					.read()
					.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
				match modifications_lock.get(key) {
					Some(value) => return Ok(value.clone()),
					None if deleted_prefixes_lock
						.iter()
						.any(|prefix| key.starts_with(prefix.as_slice())) =>
					{
						return Ok(None);
					},
					_ => (),
				}
			}
			// Not in modifications, query remote at first_forked_block
			return Ok(self.parent.get(self.first_forked_block_hash, key).await?.map(Arc::new));
		}

		// Case 2: Historical block after fork - check local_storage table
		if block_number > self.first_forked_block_number &&
			block_number < latest_block_number &&
			let Some(cached) = self.parent.cache().get_local_storage(block_number, key).await?
		{
			return Ok(cached.map(Arc::new));
		}

		// Case 3: Block before or at fork point - fetch and cache block if needed
		let block = self.fetch_and_cache_block_if_needed(block_number).await?;

		if let Some(block_row) = block {
			let block_hash = H256::from_slice(&block_row.hash);
			Ok(self.parent.get(block_hash, key).await?.map(Arc::new))
		} else {
			// Block not found
			Ok(None)
		}
	}

	/// Set a storage value locally.
	///
	/// # Arguments
	/// * `key` - The storage key to set
	/// * `value` - The value to set, or `None` to mark as deleted
	///
	/// # Returns
	/// * `Ok(())` - Value was set successfully
	/// * `Err(_)` - Lock error
	///
	/// # Behavior
	/// - Does not affect the parent layer or underlying cache
	/// - Overwrites any previous local modification for this key
	/// - Passing `None` marks the key as explicitly deleted (different from never set)
	pub fn set(&self, key: &[u8], value: Option<&[u8]>) -> Result<(), LocalStorageError> {
		let mut modifications_lock =
			self.modifications.write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		modifications_lock.insert(key.to_vec(), value.map(|value| Arc::new(value.to_vec())));

		Ok(())
	}

	/// Get multiple storage values in a batch.
	///
	/// # Arguments
	/// * `block_number` - The block number to query
	/// * `keys` - Slice of storage keys to fetch (as byte slices)
	///
	/// # Returns
	/// * `Ok(vec)` - Vector of optional values, in the same order as input keys
	/// * `Err(_)` - Lock error or parent layer error
	///
	/// # Behavior
	/// Storage lookup strategy based on block_number (same as `get`):
	/// 1. If `block_number == latest_block_number`: Check modifications HashMap, then remote at
	///    first_forked_block
	/// 2. If `first_forked_block_number < block_number < latest_block_number`: Check local_storage
	///    table
	/// 3. Otherwise: Check remote provider directly (fetches block_hash from blocks table)
	pub async fn get_batch(
		&self,
		block_number: u32,
		keys: &[&[u8]],
	) -> Result<Vec<Option<SharedValue>>, LocalStorageError> {
		if keys.is_empty() {
			return Ok(vec![]);
		}

		let latest_block_number = self.get_latest_block_number();

		// Case 1: Query for latest block - check modifications, then remote at first_forked_block
		if block_number == latest_block_number {
			let mut results: Vec<Option<SharedValue>> = Vec::with_capacity(keys.len());
			let mut parent_keys: Vec<&[u8]> = Vec::new();
			let mut parent_indices: Vec<usize> = Vec::new();

			{
				let modifications_lock = self
					.modifications
					.read()
					.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
				let deleted_prefixes_lock = self
					.deleted_prefixes
					.read()
					.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

				for (i, key) in keys.iter().enumerate() {
					if let Some(value) = modifications_lock.get(*key) {
						results.push(value.clone());
					} else if deleted_prefixes_lock
						.iter()
						.any(|prefix| key.starts_with(prefix.as_slice()))
					{
						results.push(None);
					} else {
						results.push(None); // Placeholder
						parent_keys.push(*key);
						parent_indices.push(i);
					}
				}
			}

			// Fetch missing keys from remote at first_forked_block
			if !parent_keys.is_empty() {
				let parent_values =
					self.parent.get_batch(self.first_forked_block_hash, &parent_keys).await?;
				for (i, parent_value) in parent_values.into_iter().enumerate() {
					let result_idx = parent_indices[i];
					results[result_idx] = parent_value.map(Arc::new);
				}
			}

			return Ok(results);
		}

		// Case 2: Historical block after fork - check local_storage table
		if block_number > self.first_forked_block_number && block_number < latest_block_number {
			let cached_values =
				self.parent.cache().get_local_storage_batch(block_number, keys).await?;
			return Ok(cached_values.into_iter().map(|v| v.flatten().map(Arc::new)).collect());
		}

		// Case 3: Block before or at fork point - fetch and cache block if needed
		let block = self.fetch_and_cache_block_if_needed(block_number).await?;

		if let Some(block_row) = block {
			let block_hash = H256::from_slice(&block_row.hash);
			let parent_values = self.parent.get_batch(block_hash, keys).await?;
			Ok(parent_values.into_iter().map(|v| v.map(Arc::new)).collect())
		} else {
			// Block not found - return None for all keys
			Ok(vec![None; keys.len()])
		}
	}

	/// Set multiple storage values locally in a batch.
	///
	/// # Arguments
	/// * `entries` - Slice of (key, value) pairs to set
	///
	/// # Returns
	/// * `Ok(())` - All values were set successfully
	/// * `Err(_)` - Lock error
	///
	/// # Behavior
	/// - Does not affect the parent layer or underlying cache
	/// - Overwrites any previous local modifications for the given keys
	/// - `None` values mark keys as explicitly deleted
	/// - More efficient than calling `set()` multiple times due to single lock acquisition
	pub fn set_batch(&self, entries: &[(&[u8], Option<&[u8]>)]) -> Result<(), LocalStorageError> {
		if entries.is_empty() {
			return Ok(());
		}

		let mut modifications_lock =
			self.modifications.write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		for (key, value) in entries {
			modifications_lock.insert(key.to_vec(), value.map(|v| Arc::new(v.to_vec())));
		}

		Ok(())
	}

	/// Delete all keys matching a prefix.
	///
	/// # Arguments
	/// * `prefix` - The prefix to match for deletion
	///
	/// # Returns
	/// * `Ok(())` - Prefix was marked as deleted successfully
	/// * `Err(_)` - Lock error
	///
	/// # Behavior
	/// - Removes all locally modified keys that start with the prefix
	/// - Marks the prefix as deleted, affecting future `get()` calls
	/// - Keys in the parent layer matching this prefix will return `None` after this call
	pub fn delete_prefix(&self, prefix: &[u8]) -> Result<(), LocalStorageError> {
		let mut modifications_lock =
			self.modifications.write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let mut deleted_prefixes_lock = self
			.deleted_prefixes
			.write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		// Remove all keys starting with the prefix using retain
		modifications_lock.retain(|key, _| !key.starts_with(prefix));

		// Add prefix to deleted_prefixes
		deleted_prefixes_lock.push(prefix.to_vec());

		Ok(())
	}

	/// Check if a prefix has been deleted.
	///
	/// # Arguments
	/// * `prefix` - The prefix to check
	///
	/// # Returns
	/// * `Ok(true)` - Prefix has been deleted via [`delete_prefix`](Self::delete_prefix)
	/// * `Ok(false)` - Prefix has not been deleted
	/// * `Err(_)` - Lock error
	pub fn is_deleted(&self, prefix: &[u8]) -> Result<bool, LocalStorageError> {
		let deleted_prefixes_lock = self
			.deleted_prefixes
			.read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		Ok(deleted_prefixes_lock
			.iter()
			.any(|deleted_prefix| deleted_prefix.as_slice() == prefix))
	}

	/// Get all local modifications as a vector.
	///
	/// # Returns
	/// * `Ok(vec)` - Vector of (key, value) pairs representing all local changes
	/// * `Err(_)` - Lock error
	///
	/// # Behavior
	/// - Returns only locally modified keys, not the full state
	/// - `None` values indicate keys that were explicitly deleted
	/// - Does not include keys deleted via prefix deletion
	pub fn diff(&self) -> Result<DiffLocalStorage, LocalStorageError> {
		let modifications_lock =
			self.modifications.read().map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		Ok(modifications_lock
			.iter()
			.map(|(key, value)| (key.clone(), value.clone()))
			.collect())
	}

	/// Merge modifications from another layer into this one.
	///
	/// # Arguments
	/// * `other` - The layer whose modifications to merge
	///
	/// # Returns
	/// * `Ok(())` - Merge completed successfully
	/// * `Err(_)` - Lock error
	///
	/// # Behavior
	/// - All modifications from `other` are copied to `self`
	/// - If both layers have modifications for the same key, `other`'s value wins
	/// - Deleted prefixes from `other` are added to `self`, avoiding duplicates
	/// - Does not modify `other` in any way
	pub fn merge(&self, other: &LocalStorageLayer) -> Result<(), LocalStorageError> {
		let mut self_modifications =
			self.modifications.write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let other_modifications =
			other.modifications.read().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let mut self_deleted_prefixes = self
			.deleted_prefixes
			.write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let other_deleted_prefixes = other
			.deleted_prefixes
			.read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		// Merge modifications (other's values take precedence)
		for (key, value) in other_modifications.iter() {
			self_modifications.insert(key.clone(), value.clone());
		}

		// Extend deleted prefixes, avoiding duplicates
		for prefix in other_deleted_prefixes.iter() {
			if !self_deleted_prefixes.iter().any(|p| p.as_slice() == prefix.as_slice()) {
				self_deleted_prefixes.push(prefix.clone());
			}
		}

		Ok(())
	}

	/// Create a child layer for nested modifications.
	///
	/// # Returns
	/// A cloned `LocalStorageLayer` that shares the same parent and state.
	///
	/// # Behavior
	/// - The child shares the same `modifications` and `deleted_prefixes` via `Arc`
	/// - Changes in the child affect the parent and vice versa
	/// - Useful for creating temporary scopes that can be discarded
	///
	/// # Note
	/// This is currently a simple clone. In the future, this may be updated to create
	/// true isolated child layers with proper parent-child relationships.
	pub fn child(&self) -> LocalStorageLayer {
		self.clone()
	}

	/// Commit all modifications to the local_storage table in the cache, leaving that state as
	/// latest_block_number height.
	///
	/// # Returns
	/// * `Ok(())` - All modifications were successfully committed to the cache
	/// * `Err(_)` - Lock error or cache error
	///
	/// # Behavior
	/// - Writes all locally modified key-value pairs to the local_storage table
	/// - The modifications HashMap remains intact and available after commit
	/// - Uses the parent layer's cache to persist the data
	/// - Uses batch operation for efficiency
	/// - Increases the latest block number
	pub async fn commit(&mut self) -> Result<(), LocalStorageError> {
		let new_latest_block =
			self.latest_block_number.checked_add(1).ok_or(LocalStorageError::Arithmetic)?;

		// Collect all modifications into a batch
		let diff = self.diff()?;

		// Write all modifications to the local_storage table in a batch
		if !diff.is_empty() {
			let entries = diff
				.iter()
				.map(|(key, shared_value)| {
					(key.as_slice(), shared_value.as_deref().map(|vec| vec.as_slice()))
				})
				.collect::<Vec<_>>();
			self.parent
				.cache()
				.set_local_storage_batch(self.latest_block_number, entries.as_slice())
				.await?;
		}

		self.latest_block_number = new_latest_block;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ForkRpcClient, RemoteStorageLayer, StorageCache};
	use pop_common::test_env::TestNode;
	use std::time::Duration;
	use subxt::ext::codec::Decode;
	use url::Url;

	/// System::Number storage key: twox128("System") ++ twox128("Number")
	const SYSTEM_NUMBER_KEY: &str =
		"26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac";

	/// System::ParentHash storage key: twox128("System") ++ twox128("ParentHash")
	const SYSTEM_PARENT_HASH_KEY: &str =
		"26aa394eea5630e07c48ae0c9558cef734abf5cb34d6244378cddbf18e849d96";

	/// System pallet prefix: twox128("System")
	const SYSTEM_PALLET_PREFIX: &str = "26aa394eea5630e07c48ae0c9558cef7";

	/// Helper struct to hold the test node and layers together.
	struct TestContext {
		#[allow(dead_code)]
		node: TestNode,
		remote: RemoteStorageLayer,
		block_hash: H256,
		block_number: u32,
	}

	async fn create_test_context() -> TestContext {
		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let endpoint: Url = node.ws_url().parse().unwrap();
		let rpc = ForkRpcClient::connect(&endpoint).await.unwrap();
		let block_hash = rpc.finalized_head().await.unwrap();
		let header = rpc.header(block_hash).await.unwrap();
		let block_number = header.number;
		let cache = StorageCache::in_memory().await.unwrap();
		let remote = RemoteStorageLayer::new(rpc, cache);

		TestContext { node, remote, block_hash, block_number }
	}

	/// Helper to create a LocalStorageLayer with proper block hash and number
	fn create_layer(ctx: &TestContext) -> LocalStorageLayer {
		LocalStorageLayer::new(ctx.remote.clone(), ctx.block_number, ctx.block_hash)
	}

	// Tests for new()
	#[tokio::test(flavor = "multi_thread")]
	async fn new_creates_empty_layer() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote.clone(), ctx.block_number, ctx.block_hash);

		// Verify empty modifications
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0, "New layer should have no modifications");
	}

	// Tests for get()
	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_local_modification() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote.clone(), ctx.block_number, ctx.block_hash);

		let key = b"test_key";
		let value = b"test_value";

		// Set a local value
		layer.set(key, Some(value)).unwrap();

		// Get should return the local value
		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(value.as_slice().to_vec())),
			"get() should return local modification"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_none_for_deleted_value() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote.clone(), ctx.block_number, ctx.block_hash);

		let key = b"deleted_key";

		// Set to None (explicit deletion)
		layer.set(key, None).unwrap();

		// Get should return None
		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert!(result.is_none(), "get() should return None for deleted key");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_none_for_deleted_prefix() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote.clone(), ctx.block_number, ctx.block_hash);

		let prefix = b"prefix_";
		let key = b"prefix_key";

		// Delete prefix
		layer.delete_prefix(prefix).unwrap();

		// Get should return None for key matching prefix
		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert!(result.is_none(), "get() should return None for deleted prefix");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_falls_back_to_parent() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote.clone(), ctx.block_number, ctx.block_hash);

		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Get without local modification - should fetch from parent
		let result = layer.get(ctx.block_number, &key).await.unwrap();
		assert!(
			result.is_some(),
			"get() should fall back to parent layer when key not modified locally"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_overrides_parent() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote.clone(), ctx.block_number, ctx.block_hash);

		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let local_value = b"local_override";

		// Get parent value first
		let parent_value = layer.get(ctx.block_number, &key).await.unwrap();
		assert!(parent_value.is_some());

		// Set local value
		layer.set(&key, Some(local_value)).unwrap();

		// Get should return local value, not parent
		let result = layer.get(ctx.block_number, &key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(local_value.as_slice().to_vec())),
			"get() should return local value over parent value"
		);
		assert_ne!(result, parent_value);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_none_for_nonexistent_key() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key = b"nonexistent_key_12345";

		// Get should return None for nonexistent key
		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert!(result.is_none(), "get() should return None for nonexistent key");
	}

	// Tests for set()
	#[tokio::test(flavor = "multi_thread")]
	async fn set_stores_value() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key = b"key";
		let value = b"value";

		layer.set(key, Some(value)).unwrap();

		// Verify via get
		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert_eq!(result, Some(Arc::new(value.as_slice().to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_none_marks_as_deleted() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key = b"key";

		layer.set(key, None).unwrap();

		// Verify via get
		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert!(result.is_none());

		// Verify in diff
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 1);
		assert!(diff[0].1.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_overwrites_previous_value() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key, Some(value1)).unwrap();
		layer.set(key, Some(value2)).unwrap();

		// Should have the second value
		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_multiple_keys() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set_batch(&[(key1, Some(value1)), (key2, Some(value2))]).unwrap();

		// Both should be retrievable
		let results = layer.get_batch(ctx.block_number, &[key1, key2]).await.unwrap();
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(value1.as_slice()));
		assert_eq!(results[1].as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	// Tests for get_batch()
	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_empty_keys() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let results = layer.get_batch(ctx.block_number, &[]).await.unwrap();
		assert_eq!(results.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_local_modifications() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set_batch(&[(key1, Some(value1)), (key2, Some(value2))]).unwrap();

		let results = layer.get_batch(ctx.block_number, &[key1, key2]).await.unwrap();
		assert_eq!(results.len(), 2);
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(value1.as_slice()));
		assert_eq!(results[1].as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_none_for_deleted() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";

		layer.set_batch(&[(key1, Some(b"val")), (key2, None)]).unwrap();

		let results = layer.get_batch(ctx.block_number, &[key1, key2]).await.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_none_for_deleted_prefix() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";
		let key1 = b"prefix_key1";
		let key2 = b"prefix_key2";

		layer.delete_prefix(prefix).unwrap();

		let results = layer.get_batch(ctx.block_number, &[key1, key2]).await.unwrap();
		assert!(results[0].is_none());
		assert!(results[1].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_falls_back_to_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

		let results = layer
			.get_batch(ctx.block_number, &[key1.as_slice(), key2.as_slice()])
			.await
			.unwrap();
		assert!(results[0].is_some(), "Should fetch from parent");
		assert!(results[1].is_some(), "Should fetch from parent");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_local_overrides_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
		let local_value = b"local_override";

		// Set one key locally
		layer.set(&key1, Some(local_value)).unwrap();

		let results = layer
			.get_batch(ctx.block_number, &[key1.as_slice(), key2.as_slice()])
			.await
			.unwrap();
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(local_value.as_slice()));
		assert!(results[1].is_some(), "Should fetch from parent");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_mixed_sources() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let local_key = b"local_key";
		let remote_key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let deleted_key = b"deleted_key";
		let nonexistent_key = b"nonexistent_key";

		layer.set(local_key, Some(b"local_value")).unwrap();
		layer.set(deleted_key, None).unwrap();

		let results = layer
			.get_batch(
				ctx.block_number,
				&[local_key, remote_key.as_slice(), deleted_key, nonexistent_key],
			)
			.await
			.unwrap();

		assert_eq!(results.len(), 4);
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(b"local_value".as_slice()));
		assert!(results[1].is_some()); // from parent
		assert!(results[2].is_none()); // deleted
		assert!(results[3].is_none()); // nonexistent
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_maintains_order() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";
		let key3 = b"key3";
		let value1 = b"value1";
		let value2 = b"value2";
		let value3 = b"value3";

		layer
			.set_batch(&[(key1, Some(value1)), (key2, Some(value2)), (key3, Some(value3))])
			.unwrap();

		// Request in different order
		let results = layer.get_batch(ctx.block_number, &[key3, key1, key2]).await.unwrap();
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(value3.as_slice()));
		assert_eq!(results[1].as_ref().map(|v| v.as_slice()), Some(value1.as_slice()));
		assert_eq!(results[2].as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	// Tests for set_batch()
	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_empty_entries() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		layer.set_batch(&[]).unwrap();

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_stores_multiple_values() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";
		let key3 = b"key3";
		let value1 = b"value1";
		let value2 = b"value2";
		let value3 = b"value3";

		layer
			.set_batch(&[(key1, Some(value1)), (key2, Some(value2)), (key3, Some(value3))])
			.unwrap();

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 3);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_with_deletions() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";

		layer.set_batch(&[(key1, Some(value1)), (key2, None)]).unwrap();

		let results = layer.get_batch(ctx.block_number, &[key1, key2]).await.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_overwrites_previous_values() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key, Some(value1)).unwrap();
		layer.set_batch(&[(key, Some(value2))]).unwrap();

		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_duplicate_keys_last_wins() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		// Set same key twice in one batch - last should win
		layer.set_batch(&[(key, Some(value1)), (key, Some(value2))]).unwrap();

		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	// Tests for delete_prefix()
	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_removes_matching_keys() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";
		let key1 = b"prefix_key1";
		let key2 = b"prefix_key2";
		let key3 = b"other_key";

		// Set values
		layer.set(key1, Some(b"val1")).unwrap();
		layer.set(key2, Some(b"val2")).unwrap();
		layer.set(key3, Some(b"val3")).unwrap();

		// Delete prefix
		layer.delete_prefix(prefix).unwrap();

		// Matching keys should be gone from modifications
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 1, "Only non-matching key should remain");
		assert_eq!(diff[0].0, key3);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_blocks_parent_reads() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();
		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Verify key exists in parent
		let before = layer.get(ctx.block_number, &key).await.unwrap();
		assert!(before.is_some());

		// Delete prefix
		layer.delete_prefix(&prefix).unwrap();

		// Should return None now
		let after = layer.get(ctx.block_number, &key).await.unwrap();
		assert!(after.is_none(), "delete_prefix() should block parent reads");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_adds_to_deleted_prefixes() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";

		layer.delete_prefix(prefix).unwrap();

		// Should be marked as deleted
		assert!(layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_with_empty_prefix() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";

		layer.set(key1, Some(b"val1")).unwrap();
		layer.set(key2, Some(b"val2")).unwrap();

		// Delete empty prefix (matches everything)
		layer.delete_prefix(b"").unwrap();

		// All modifications should be removed
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0, "Empty prefix should delete all modifications");
	}

	// Tests for is_deleted()
	#[tokio::test(flavor = "multi_thread")]
	async fn is_deleted_returns_false_initially() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";

		assert!(!layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn is_deleted_returns_true_after_delete() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";

		layer.delete_prefix(prefix).unwrap();

		assert!(layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn is_deleted_exact_match_only() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix1 = b"prefix_";
		let prefix2 = b"prefix_other";

		layer.delete_prefix(prefix1).unwrap();

		assert!(layer.is_deleted(prefix1).unwrap());
		assert!(!layer.is_deleted(prefix2).unwrap());
	}

	// Tests for diff()
	#[tokio::test(flavor = "multi_thread")]
	async fn diff_returns_empty_initially() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn diff_returns_all_modifications() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key1, Some(value1)).unwrap();
		layer.set(key2, Some(value2)).unwrap();

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 2);
		assert!(
			diff.iter()
				.any(|(k, v)| k == key1 &&
					v.as_ref().map(|v| v.as_slice()) == Some(value1.as_slice()))
		);
		assert!(
			diff.iter()
				.any(|(k, v)| k == key2 &&
					v.as_ref().map(|v| v.as_slice()) == Some(value2.as_slice()))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn diff_includes_deletions() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key = b"deleted";

		layer.set(key, None).unwrap();

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 1);
		assert_eq!(diff[0].0, key);
		assert!(diff[0].1.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn diff_excludes_prefix_deleted_keys() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";
		let key = b"prefix_key";

		layer.set(key, Some(b"value")).unwrap();
		layer.delete_prefix(prefix).unwrap();

		// Key should be removed from modifications
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0, "diff() should not include prefix-deleted keys");
	}

	// Tests for merge()
	#[tokio::test(flavor = "multi_thread")]
	async fn merge_combines_modifications() {
		let ctx = create_test_context().await;

		let layer1 = create_layer(&ctx);
		let layer2 = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer1.set(key1, Some(value1)).unwrap();
		layer2.set(key2, Some(value2)).unwrap();

		layer1.merge(&layer2).unwrap();

		// layer1 should have both
		let diff = layer1.diff().unwrap();
		assert_eq!(diff.len(), 2);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn merge_other_takes_precedence() {
		let ctx = create_test_context().await;

		let layer1 = create_layer(&ctx);
		let layer2 = create_layer(&ctx);

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer1.set(key, Some(value1)).unwrap();
		layer2.set(key, Some(value2)).unwrap();

		layer1.merge(&layer2).unwrap();

		// layer1 should have layer2's value
		let result = layer1.get(ctx.block_number, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn merge_combines_deleted_prefixes() {
		let ctx = create_test_context().await;

		let layer1 = create_layer(&ctx);
		let layer2 = create_layer(&ctx);

		let prefix1 = b"prefix1_";
		let prefix2 = b"prefix2_";

		layer1.delete_prefix(prefix1).unwrap();
		layer2.delete_prefix(prefix2).unwrap();

		layer1.merge(&layer2).unwrap();

		// Both should be deleted in layer1
		assert!(layer1.is_deleted(prefix1).unwrap());
		assert!(layer1.is_deleted(prefix2).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn merge_avoids_duplicate_prefixes() {
		let ctx = create_test_context().await;

		let layer1 = create_layer(&ctx);
		let layer2 = create_layer(&ctx);

		let prefix = b"prefix_";

		layer1.delete_prefix(prefix).unwrap();
		layer2.delete_prefix(prefix).unwrap();

		let before_count = layer1.deleted_prefixes.try_read().unwrap().len();

		layer1.merge(&layer2).unwrap();

		let after_count = layer1.deleted_prefixes.try_read().unwrap().len();
		assert_eq!(before_count, after_count, "merge() should avoid duplicate prefixes");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn merge_does_not_modify_other() {
		let ctx = create_test_context().await;

		let layer1 = create_layer(&ctx);
		let layer2 = create_layer(&ctx);

		let key = b"key";
		let value = b"value";

		layer1.set(key, Some(value)).unwrap();

		let before_diff = layer2.diff().unwrap();

		layer1.merge(&layer2).unwrap();

		let after_diff = layer2.diff().unwrap();
		assert_eq!(before_diff.len(), after_diff.len(), "merge() should not modify other");
	}

	// Tests for child()
	#[tokio::test(flavor = "multi_thread")]
	async fn child_shares_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let child = layer.child();

		// Both should be able to read from parent
		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let parent_result = layer.get(ctx.block_number, &key).await.unwrap();
		let child_result = child.get(ctx.block_number, &key).await.unwrap();

		assert_eq!(parent_result, child_result);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn child_shares_modifications() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let key = b"key";
		let value = b"value";

		layer.set(key, Some(value)).unwrap();

		let child = layer.child();

		// Child should see parent's modification
		let result = child.get(ctx.block_number, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn child_modifications_affect_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let child = layer.child();

		let key = b"key";
		let value = b"value";

		child.set(key, Some(value)).unwrap();

		// Parent should see child's modification (shared state)
		let result = layer.get(ctx.block_number, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value.as_slice()));
	}

	// Tests for commit()
	#[tokio::test(flavor = "multi_thread")]
	async fn commit_writes_to_cache() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let key1 = b"commit_key1";
		let key2 = b"commit_key2";
		let value1 = b"commit_value1";
		let value2 = b"commit_value2";

		// Set local modifications
		layer.set(key1, Some(value1)).unwrap();
		layer.set(key2, Some(value2)).unwrap();

		// Verify not in cache yet
		assert!(
			ctx.remote
				.cache()
				.get_local_storage(ctx.block_number, key1)
				.await
				.unwrap()
				.is_none()
		);
		assert!(
			ctx.remote
				.cache()
				.get_local_storage(ctx.block_number, key2)
				.await
				.unwrap()
				.is_none()
		);

		// Commit
		layer.commit().await.unwrap();

		// Verify now in cache at the block_number it was committed to
		let cached1 = ctx.remote.cache().get_local_storage(ctx.block_number, key1).await.unwrap();
		let cached2 = ctx.remote.cache().get_local_storage(ctx.block_number, key2).await.unwrap();

		assert_eq!(cached1, Some(Some(value1.to_vec())));
		assert_eq!(cached2, Some(Some(value2.to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_preserves_modifications() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let key = b"preserve_key";
		let value = b"preserve_value";

		// Set and commit
		layer.set(key, Some(value)).unwrap();
		layer.commit().await.unwrap();

		// Modifications should still be in local layer
		let local_result = layer.get(ctx.block_number + 1, key).await.unwrap();
		assert_eq!(local_result.as_ref().map(|v| v.as_slice()), Some(value.as_slice()));

		// Should also be in diff
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 1);
		assert_eq!(diff[0].0, key);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_with_deletions() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let key1 = b"delete_key1";
		let key2 = b"delete_key2";
		let value = b"value";

		// Set one value and mark another as deleted
		layer.set(key1, Some(value)).unwrap();
		layer.set(key2, None).unwrap();

		// Commit
		layer.commit().await.unwrap();

		// Both should be in cache
		let cached1 = ctx.remote.cache().get_local_storage(ctx.block_number, key1).await.unwrap();
		let cached2 = ctx.remote.cache().get_local_storage(ctx.block_number, key2).await.unwrap();

		assert_eq!(cached1, Some(Some(value.to_vec())));
		assert_eq!(cached2, Some(None)); // Cached as empty
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_empty_modifications() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		// Commit with no modifications should succeed
		let result = layer.commit().await;
		assert!(result.is_ok());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_multiple_times() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let key = b"multi_block_key";
		let value = b"multi_block_value";

		// Set local modification
		layer.set(key, Some(value)).unwrap();

		// Commit multiple times - each commit increments the block number
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();

		// Both block numbers should have the value in cache
		let cached1 = ctx.remote.cache().get_local_storage(ctx.block_number, key).await.unwrap();
		let cached2 =
			ctx.remote.cache().get_local_storage(ctx.block_number + 1, key).await.unwrap();

		assert_eq!(cached1, Some(Some(value.to_vec())));
		assert_eq!(cached2, Some(Some(value.to_vec())));
	}

	// Tests for fetch_and_cache_block_if_needed (via get/get_batch for historical blocks)
	#[tokio::test(flavor = "multi_thread")]
	async fn get_historical_block() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		// Query a block that's not in cache (block 0)
		let block_number = 0u32;
		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Verify block is not in cache initially
		let cached_before = ctx.remote.cache().get_block_by_number(block_number).await.unwrap();
		assert!(cached_before.is_none());

		// Get storage from historical block
		let result = layer.get(block_number, &key).await.unwrap().unwrap();
		assert_eq!(u32::decode(&mut &result[..]).unwrap(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_historical_block() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		// Wait for some blocks to be finalized
		std::thread::sleep(Duration::from_secs(30));

		// Query a block that's not in cache
		let block_number = 1u32;
		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

		// Get storage from historical block
		let results = layer
			.get_batch(block_number, &[key1.as_slice(), key2.as_slice()])
			.await
			.unwrap();
		assert_eq!(results.len(), 2);
		assert_eq!(u32::decode(&mut &results[0].as_ref().unwrap()[..]).unwrap(), 1);
		assert_eq!(
			H256::decode(&mut &results[1].as_ref().unwrap()[..]).unwrap(),
			H256::from([0; 32])
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_non_existent_block_returns_none() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		// Query a block that doesn't exist
		let non_existent_block = u32::MAX;
		let key = b"some_key";

		let result = layer.get(non_existent_block, key).await.unwrap();
		assert!(result.is_none(), "Non-existent block should return None");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_non_existent_block_returns_none() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		// Query a block that doesn't exist
		let non_existent_block = u32::MAX;
		let keys: Vec<&[u8]> = vec![b"key1", b"key2"];

		let results = layer.get_batch(non_existent_block, &keys).await.unwrap();
		assert_eq!(results.len(), 2);
		assert!(results[0].is_none(), "Non-existent block should return None");
		assert!(results[1].is_none(), "Non-existent block should return None");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_mixed_block_scenarios() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		// Wait for some blocks to be finalized
		std::thread::sleep(Duration::from_secs(30));

		// Test multiple scenarios:
		// 1. Latest block (from modifications)
		// 2. Historical block (from cache/RPC)

		let key1 = b"local_key";
		let key2 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Set a local modification
		layer.set(key1, Some(b"local_value")).unwrap();

		// Get from latest block (should hit modifications)
		let results1 = layer.get(ctx.block_number, &key1[..]).await.unwrap();
		assert_eq!(results1.as_ref().map(|v| v.as_slice()), Some(b"local_value".as_slice()));

		// Get from historical block (should fetch and cache block)
		let historical_block = 0u32;
		let results2 = layer.get(historical_block, key2.as_slice()).await.unwrap().unwrap();
		assert_eq!(u32::decode(&mut &results2[..]).unwrap(), 0);

		// Commit block modifications
		layer.commit().await.unwrap();

		layer.set(key1, Some(b"local_value_2")).unwrap();

		let result_previous_block = layer.get(ctx.block_number, &key1[..]).await.unwrap();
		let result_latest_block = layer.get(layer.latest_block_number, &key1[..]).await.unwrap();

		assert_eq!(*result_previous_block.unwrap(), b"local_value".to_vec());
		assert_eq!(*result_latest_block.unwrap(), b"local_value_2".to_vec());
	}
}
