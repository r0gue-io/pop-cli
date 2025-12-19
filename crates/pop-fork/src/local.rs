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

use crate::{error::LocalStorageError, remote::RemoteStorageLayer};
use std::{
	collections::HashMap,
	sync::{Arc, RwLock},
};

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
/// # Cloning
///
/// `LocalStorageLayer` is cheap to clone. The underlying modifications and
/// deleted prefixes use `Arc<RwLock<_>>`, so clones share the same state.
///
/// # Thread Safety
///
/// The layer is `Send + Sync` and can be shared across async tasks. All
/// operations use `try_read`/`try_write` to avoid blocking.
#[derive(Clone, Debug)]
pub struct LocalStorageLayer {
	parent: RemoteStorageLayer,
	modifications: Arc<RwLock<Modifications>>,
	deleted_prefixes: Arc<RwLock<DeletedPrefixes>>,
}

impl LocalStorageLayer {
	/// Create a new local storage layer.
	///
	/// # Arguments
	/// * `parent` - The remote storage layer to use as the base state
	///
	/// # Returns
	/// A new `LocalStorageLayer` with no modifications.
	pub fn new(parent: RemoteStorageLayer) -> Self {
		Self {
			parent,
			modifications: Arc::new(RwLock::new(HashMap::new())),
			deleted_prefixes: Arc::new(RwLock::new(Vec::new())),
		}
	}

	/// Get a storage value, checking local modifications first.
	///
	/// # Arguments
	/// * `key` - The storage key to fetch
	///
	/// # Returns
	/// * `Ok(Some(value))` - Value exists (either modified locally or in parent)
	/// * `Ok(None)` - Key doesn't exist or was deleted via prefix deletion
	/// * `Err(_)` - Lock error or parent layer error
	///
	/// # Behavior
	/// 1. Checks local modifications first - returns immediately if found
	/// 2. Checks if key matches any deleted prefix - returns `None` if so
	/// 3. Falls back to querying the parent layer if not modified locally
	pub async fn get(&self, key: &[u8]) -> Result<Option<SharedValue>, LocalStorageError> {
		{
			let modifications_lock = self
				.modifications
				.try_read()
				.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
			let deleted_prefixes_lock = self
				.deleted_prefixes
				.try_read()
				.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
			match modifications_lock.get(key) {
				Some(value) => return Ok(value.clone()),
				None if deleted_prefixes_lock.iter().any(|prefix| key.starts_with(prefix.as_slice())) =>
					return Ok(None),
				_ => (),
			}
		}

		Ok(self.parent.get(key).await?.map(Arc::new))
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
		let mut modifications_lock = self
			.modifications
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		modifications_lock.insert(key.to_vec(), value.map(|value| Arc::new(value.to_vec())));

		Ok(())
	}

	/// Get multiple storage values in a batch.
	///
	/// # Arguments
	/// * `keys` - Slice of storage keys to fetch (as byte slices)
	///
	/// # Returns
	/// * `Ok(vec)` - Vector of optional values, in the same order as input keys
	/// * `Err(_)` - Lock error or parent layer error
	///
	/// # Behavior
	/// - For each key, checks local modifications first
	/// - Falls back to parent layer for unmodified keys
	/// - Returns `None` for keys that are deleted or match deleted prefixes
	/// - More efficient than calling `get()` multiple times due to lock reuse
	pub async fn get_batch(
		&self,
		keys: &[&[u8]],
	) -> Result<Vec<Option<SharedValue>>, LocalStorageError> {
		if keys.is_empty() {
			return Ok(vec![]);
		}

		// Separate keys into locally available and needs parent fetch
		let mut results: Vec<Option<SharedValue>> = Vec::with_capacity(keys.len());
		let mut parent_keys: Vec<&[u8]> = Vec::new();
		let mut parent_indices: Vec<usize> = Vec::new();

		{
			let modifications_lock = self
				.modifications
				.try_read()
				.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
			let deleted_prefixes_lock = self
				.deleted_prefixes
				.try_read()
				.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

			for (i, key) in keys.iter().enumerate() {
				if let Some(value) = modifications_lock.get(*key) {
					// Key is locally modified
					results.push(value.clone());
				} else if deleted_prefixes_lock
					.iter()
					.any(|prefix| key.starts_with(prefix.as_slice()))
				{
					// Key matches a deleted prefix
					results.push(None);
				} else {
					// Need to fetch from parent
					results.push(None); // Placeholder
					parent_keys.push(*key);
					parent_indices.push(i);
				}
			}
		}

		// Fetch missing keys from parent
		if !parent_keys.is_empty() {
			let parent_values = self.parent.get_batch(&parent_keys).await?;

			// Fill in parent values
			for (i, parent_value) in parent_values.into_iter().enumerate() {
				let result_idx = parent_indices[i];
				results[result_idx] = parent_value.map(Arc::new);
			}
		}

		Ok(results)
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

		let mut modifications_lock = self
			.modifications
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

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
		let mut modifications_lock = self
			.modifications
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let mut deleted_prefixes_lock = self
			.deleted_prefixes
			.try_write()
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
			.try_read()
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
		let modifications_lock = self
			.modifications
			.try_read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

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
		let mut self_modifications = self
			.modifications
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let other_modifications = other
			.modifications
			.try_read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let mut self_deleted_prefixes = self
			.deleted_prefixes
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let other_deleted_prefixes = other
			.deleted_prefixes
			.try_read()
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
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ForkRpcClient, RemoteStorageLayer, StorageCache};
	use pop_common::test_env::TestNode;
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
	}

	async fn create_test_context() -> TestContext {
		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let endpoint: Url = node.ws_url().parse().unwrap();
		let rpc = ForkRpcClient::connect(&endpoint).await.unwrap();
		let cache = StorageCache::in_memory().await.unwrap();
		let block_hash = rpc.finalized_head().await.unwrap();
		let remote = RemoteStorageLayer::new(rpc, cache, block_hash);

		TestContext { node, remote }
	}

	// Tests for new()
	#[tokio::test(flavor = "multi_thread")]
	async fn new_creates_empty_layer() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		// Verify empty modifications
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0, "New layer should have no modifications");
	}

	// Tests for get()
	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_local_modification() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"test_key";
		let value = b"test_value";

		// Set a local value
		layer.set(key, Some(value)).unwrap();

		// Get should return the local value
		let result = layer.get(key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(value.as_slice().to_vec())),
			"get() should return local modification"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_none_for_deleted_value() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"deleted_key";

		// Set to None (explicit deletion)
		layer.set(key, None).unwrap();

		// Get should return None
		let result = layer.get(key).await.unwrap();
		assert!(result.is_none(), "get() should return None for deleted key");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_none_for_deleted_prefix() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let prefix = b"prefix_";
		let key = b"prefix_key";

		// Delete prefix
		layer.delete_prefix(prefix).unwrap();

		// Get should return None for key matching prefix
		let result = layer.get(key).await.unwrap();
		assert!(result.is_none(), "get() should return None for deleted prefix");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_falls_back_to_parent() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Get without local modification - should fetch from parent
		let result = layer.get(&key).await.unwrap();
		assert!(
			result.is_some(),
			"get() should fall back to parent layer when key not modified locally"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_overrides_parent() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let local_value = b"local_override";

		// Get parent value first
		let parent_value = layer.get(&key).await.unwrap();
		assert!(parent_value.is_some());

		// Set local value
		layer.set(&key, Some(local_value)).unwrap();

		// Get should return local value, not parent
		let result = layer.get(&key).await.unwrap();
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
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"nonexistent_key_12345";

		// Get should return None for nonexistent key
		let result = layer.get(key).await.unwrap();
		assert!(result.is_none(), "get() should return None for nonexistent key");
	}

	// Tests for set()
	#[tokio::test(flavor = "multi_thread")]
	async fn set_stores_value() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"key";
		let value = b"value";

		layer.set(key, Some(value)).unwrap();

		// Verify via get
		let result = layer.get(key).await.unwrap();
		assert_eq!(result, Some(Arc::new(value.as_slice().to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_none_marks_as_deleted() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"key";

		layer.set(key, None).unwrap();

		// Verify via get
		let result = layer.get(key).await.unwrap();
		assert!(result.is_none());

		// Verify in diff
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 1);
		assert!(diff[0].1.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_overwrites_previous_value() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key, Some(value1)).unwrap();
		layer.set(key, Some(value2)).unwrap();

		// Should have the second value
		let result = layer.get(key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_multiple_keys() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set_batch(&[(key1, Some(value1)), (key2, Some(value2))]).unwrap();

		// Both should be retrievable
		let results = layer.get_batch(&[key1, key2]).await.unwrap();
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(value1.as_slice()));
		assert_eq!(results[1].as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	// Tests for get_batch()
	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_empty_keys() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let results = layer.get_batch(&[]).await.unwrap();
		assert_eq!(results.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_local_modifications() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set_batch(&[(key1, Some(value1)), (key2, Some(value2))]).unwrap();

		let results = layer.get_batch(&[key1, key2]).await.unwrap();
		assert_eq!(results.len(), 2);
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(value1.as_slice()));
		assert_eq!(results[1].as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_none_for_deleted() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key1 = b"key1";
		let key2 = b"key2";

		layer.set_batch(&[(key1, Some(b"val")), (key2, None)]).unwrap();

		let results = layer.get_batch(&[key1, key2]).await.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_none_for_deleted_prefix() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let prefix = b"prefix_";
		let key1 = b"prefix_key1";
		let key2 = b"prefix_key2";

		layer.delete_prefix(prefix).unwrap();

		let results = layer.get_batch(&[key1, key2]).await.unwrap();
		assert!(results[0].is_none());
		assert!(results[1].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_falls_back_to_parent() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

		let results = layer.get_batch(&[key1.as_slice(), key2.as_slice()]).await.unwrap();
		assert!(results[0].is_some(), "Should fetch from parent");
		assert!(results[1].is_some(), "Should fetch from parent");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_local_overrides_parent() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
		let local_value = b"local_override";

		// Set one key locally
		layer.set(&key1, Some(local_value)).unwrap();

		let results = layer.get_batch(&[key1.as_slice(), key2.as_slice()]).await.unwrap();
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(local_value.as_slice()));
		assert!(results[1].is_some(), "Should fetch from parent");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_mixed_sources() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let local_key = b"local_key";
		let remote_key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let deleted_key = b"deleted_key";
		let nonexistent_key = b"nonexistent_key";

		layer.set(local_key, Some(b"local_value")).unwrap();
		layer.set(deleted_key, None).unwrap();

		let results = layer
			.get_batch(&[local_key, remote_key.as_slice(), deleted_key, nonexistent_key])
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
		let layer = LocalStorageLayer::new(ctx.remote);

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
		let results = layer.get_batch(&[key3, key1, key2]).await.unwrap();
		assert_eq!(results[0].as_ref().map(|v| v.as_slice()), Some(value3.as_slice()));
		assert_eq!(results[1].as_ref().map(|v| v.as_slice()), Some(value1.as_slice()));
		assert_eq!(results[2].as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	// Tests for set_batch()
	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_empty_entries() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		layer.set_batch(&[]).unwrap();

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_stores_multiple_values() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

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
		let layer = LocalStorageLayer::new(ctx.remote);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";

		layer.set_batch(&[(key1, Some(value1)), (key2, None)]).unwrap();

		let results = layer.get_batch(&[key1, key2]).await.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_overwrites_previous_values() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key, Some(value1)).unwrap();
		layer.set_batch(&[(key, Some(value2))]).unwrap();

		let result = layer.get(key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_duplicate_keys_last_wins() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		// Set same key twice in one batch - last should win
		layer.set_batch(&[(key, Some(value1)), (key, Some(value2))]).unwrap();

		let result = layer.get(key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	// Tests for delete_prefix()
	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_removes_matching_keys() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

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
		let layer = LocalStorageLayer::new(ctx.remote);

		let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();
		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Verify key exists in parent
		let before = layer.get(&key).await.unwrap();
		assert!(before.is_some());

		// Delete prefix
		layer.delete_prefix(&prefix).unwrap();

		// Should return None now
		let after = layer.get(&key).await.unwrap();
		assert!(after.is_none(), "delete_prefix() should block parent reads");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_adds_to_deleted_prefixes() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let prefix = b"prefix_";

		layer.delete_prefix(prefix).unwrap();

		// Should be marked as deleted
		assert!(layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_with_empty_prefix() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

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
		let layer = LocalStorageLayer::new(ctx.remote);

		let prefix = b"prefix_";

		assert!(!layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn is_deleted_returns_true_after_delete() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let prefix = b"prefix_";

		layer.delete_prefix(prefix).unwrap();

		assert!(layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn is_deleted_exact_match_only() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

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
		let layer = LocalStorageLayer::new(ctx.remote);

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn diff_returns_all_modifications() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

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
		let layer = LocalStorageLayer::new(ctx.remote);

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
		let layer = LocalStorageLayer::new(ctx.remote);

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

		let layer1 = LocalStorageLayer::new(ctx.remote.clone());
		let layer2 = LocalStorageLayer::new(ctx.remote.clone());

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

		let layer1 = LocalStorageLayer::new(ctx.remote.clone());
		let layer2 = LocalStorageLayer::new(ctx.remote.clone());

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer1.set(key, Some(value1)).unwrap();
		layer2.set(key, Some(value2)).unwrap();

		layer1.merge(&layer2).unwrap();

		// layer1 should have layer2's value
		let result = layer1.get(key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn merge_combines_deleted_prefixes() {
		let ctx = create_test_context().await;

		let layer1 = LocalStorageLayer::new(ctx.remote.clone());
		let layer2 = LocalStorageLayer::new(ctx.remote.clone());

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

		let layer1 = LocalStorageLayer::new(ctx.remote.clone());
		let layer2 = LocalStorageLayer::new(ctx.remote.clone());

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

		let layer1 = LocalStorageLayer::new(ctx.remote.clone());
		let layer2 = LocalStorageLayer::new(ctx.remote.clone());

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
		let layer = LocalStorageLayer::new(ctx.remote);

		let child = layer.child();

		// Both should be able to read from parent
		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let parent_result = layer.get(&key).await.unwrap();
		let child_result = child.get(&key).await.unwrap();

		assert_eq!(parent_result, child_result);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn child_shares_modifications() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let key = b"key";
		let value = b"value";

		layer.set(key, Some(value)).unwrap();

		let child = layer.child();

		// Child should see parent's modification
		let result = child.get(key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn child_modifications_affect_parent() {
		let ctx = create_test_context().await;
		let layer = LocalStorageLayer::new(ctx.remote);

		let child = layer.child();

		let key = b"key";
		let value = b"value";

		child.set(key, Some(value)).unwrap();

		// Parent should see child's modification (shared state)
		let result = layer.get(key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.as_slice()), Some(value.as_slice()));
	}
}
