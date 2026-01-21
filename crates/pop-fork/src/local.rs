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

use crate::{
	error::LocalStorageError,
	models::{BlockRow, LocalKeyRow},
	remote::RemoteStorageLayer,
};
use std::{
	collections::HashMap,
	sync::{Arc, RwLock},
};
use subxt::config::substrate::H256;

/// A value that can be shared accross different local storage layer instances
#[derive(Debug, PartialEq)]
pub struct LocalSharedValue {
	last_modification_block: u32,
	/// The actual value
	pub value: Vec<u8>,
}

impl AsRef<[u8]> for LocalSharedValue {
	/// AsRef implementation to get the value bytes of this local shared value
	fn as_ref(&self) -> &[u8] {
		self.value.as_ref()
	}
}

type SharedValue = Arc<LocalSharedValue>;
type Modifications = HashMap<Vec<u8>, Option<SharedValue>>;
type DeletedPrefixes = Vec<Vec<u8>>;
type DiffLocalStorage = Vec<(Vec<u8>, Option<SharedValue>)>;

/// Local storage layer that tracks modifications on top of a remote layer.
///
/// Provides transactional semantics: modifications are tracked locally without
/// affecting the underlying remote layer or cache. Changes can be inspected via
/// [`diff`](Self::diff).
///
/// # Block-based Storage Strategy
///
/// - `latest_block_number`: Current working block number (modifications in HashMap)
/// - Keys queried at a block higher than the last modification of that key, queried at
///   modifications HashMap
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
			latest_block_number: first_forked_block_number + 1,
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
	async fn get_block(&self, block_number: u32) -> Result<Option<BlockRow>, LocalStorageError> {
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

	/// Return the value of the specified key at height `block_number` if the value contained in the
	/// local modifications is valid at that height.
	///
	/// # Arguments
	/// - `key`
	/// - `block_number`
	fn get_local_modification(
		&self,
		key: &[u8],
		block_number: u32,
	) -> Result<Option<SharedValue>, LocalStorageError> {
		let modifications_lock =
			self.modifications.read().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		match modifications_lock.get(key) {
			local_modification @ Some(Some(shared_value))
				if shared_value.last_modification_block < block_number =>
				Ok(local_modification.expect("The match guard ensures this is Some; qed;").clone()), /* <- Cheap clone as it's Option<Arc<_>> */
			_ => Ok(None),
		}
	}

	/// Get a storage value, checking local modifications first.
	///
	/// # Arguments
	/// * `key` - The storage key to fetch
	///
	/// # Returns
	/// * `Ok(Some(value))` - Value exists (either modified locally, in local_storage, or in parent)
	/// * `Ok(None)` - Key doesn't exist or was deleted via prefix deletion
	/// * `Err(_)` - Lock error or parent layer error
	///
	/// # Behavior
	/// Storage lookup strategy based on block_number:
	/// 1. If `block_number == latest_block_number` or the key in local modifications is valid for
	///    this block: Check modifications HashMap, then remote at first_forked_block
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
			return Ok(self
				.parent
				.get(self.first_forked_block_hash, key)
				.await?
				.map(|value| LocalSharedValue {
					last_modification_block: 0, /* <- We don't care about the validity block for
					                             * this value as it came from the remote layer */
					value,
				})
				.map(Arc::new));
		}

		// Case 2: Historical block after fork such that the local modification is still valid -
		// check the local modifications map
		if let local_modification @ Ok(Some(_)) = self.get_local_modification(key, block_number) {
			return local_modification;
		}

		// Case 3: Historical block after fork such that the key isn't valid - check local_values
		// table using validity, otherwise query remote at first_forked_block
		if block_number > self.first_forked_block_number && block_number < latest_block_number {
			// Try to get value from local_values table using validity ranges
			let value = if let Some(local_value) =
				self.parent.cache().get_local_value_at_block(key, block_number).await?
			{
				local_value
			}
			// Not found in local storage, try remote at first_forked_block
			else if let Some(remote_value) =
				self.parent.get(self.first_forked_block_hash, key).await?
			{
				remote_value
			} else {
				return Ok(None);
			};

			return Ok(Some(Arc::new(LocalSharedValue {
				last_modification_block: 0, /* <- Value came from remote or cache layer */
				value,
			})));
		}

		// Case 4: Block before or at fork point
		let block = self.get_block(block_number).await?;

		if let Some(block_row) = block {
			let block_hash = H256::from_slice(&block_row.hash);
			Ok(self
				.parent
				.get(block_hash, key)
				.await?
				.map(|value| LocalSharedValue {
					last_modification_block: 0, /* <- We don't care about the validity block of
					                             * this value as it came from the remote layer */
					value,
				})
				.map(Arc::new))
		} else {
			// Block not found
			Ok(None)
		}
	}

	/// Get the next key after the given key that starts with the prefix.
	///
	/// # Arguments
	/// * `prefix` - Storage key prefix to match
	/// * `key` - The current key; returns the next key after this one
	///
	/// # Returns
	/// * `Ok(Some(key))` - The next key after `key` that starts with `prefix`
	/// * `Ok(None)` - No more keys with this prefix
	/// * `Err(_)` - Lock error or parent layer error
	///
	/// # Behavior
	/// 1. Queries the parent layer for the next key
	/// 2. Skips keys that match deleted prefixes
	/// 3. Does not consider locally modified keys (they are transient)
	///
	/// # Note
	/// This method currently delegates directly to the parent layer.
	/// Locally modified keys are not included in key enumeration since
	/// they represent uncommitted changes.
	pub async fn next_key(
		&self,
		prefix: &[u8],
		key: &[u8],
	) -> Result<Option<Vec<u8>>, LocalStorageError> {
		// Clone deleted prefixes upfront - we can't hold the lock across await points
		let deleted_prefixes = self
			.deleted_prefixes
			.read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?
			.clone();

		// Query parent and skip deleted keys
		let mut current_key = key.to_vec();
		loop {
			let next =
				self.parent.next_key(self.first_forked_block_hash, prefix, &current_key).await?;
			match next {
				Some(next_key) => {
					// Check if this key matches any deleted prefix
					if deleted_prefixes
						.iter()
						.any(|deleted| next_key.starts_with(deleted.as_slice()))
					{
						// Skip this key and continue searching
						current_key = next_key;
						continue;
					}
					return Ok(Some(next_key));
				},
				None => return Ok(None),
			}
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

		let latest_block_number = self.get_latest_block_number();

		modifications_lock.insert(
			key.to_vec(),
			value.map(|value| {
				Arc::new({
					LocalSharedValue {
						last_modification_block: latest_block_number,
						value: value.to_vec(),
					}
				})
			}),
		);

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
	/// 2. If `first_forked_block_number < block_number < latest_block_number` and the key is valid
	///    for `block_number`: Check modifications HashMap table
	/// 3. If `first_forked_block_number < block_number < latest_block_number`: Check local_storage
	///    table
	/// 4. Otherwise: Check remote provider directly (fetches block_hash from blocks table)
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
					results[result_idx] = parent_value
						.map(|value| LocalSharedValue {
							last_modification_block: 0, /* <- We don't care about the validity
							                             * block for this value as it came from
							                             * remote layer */
							value,
						})
						.map(Arc::new);
				}
			}

			return Ok(results);
		}

		// Case 2: Historical block after fork - check modifications HashMap for hot keys and
		// local_values table (using validity) for the others. Remote query for non found keys
		if block_number > self.first_forked_block_number && block_number < latest_block_number {
			let mut found_results: Vec<Option<SharedValue>> = Vec::with_capacity(keys.len());
			let mut cache_keys: Vec<&[u8]> = Vec::new();
			let mut cache_indices: Vec<usize> = Vec::new();

			for (i, key) in keys.iter().enumerate() {
				match self.get_local_modification(key, block_number) {
					local_modification @ Ok(Some(_)) => found_results.push(
						local_modification
							.expect("The match guard ensures this is Ok; qed;")
							.clone(),
					),
					_ => {
						found_results.push(None); // Placeholder
						cache_keys.push(*key);
						cache_indices.push(i);
					},
				}
			}

			if !cache_keys.is_empty() {
				// Use validity-based query to get values from local_values table
				let cached_values = self
					.parent
					.cache()
					.get_local_values_at_block_batch(&cache_keys, block_number)
					.await?;
				for (i, cache_value) in cached_values.into_iter().enumerate() {
					let result_idx = cache_indices[i];
					found_results[result_idx] = cache_value
						.map(|value| LocalSharedValue {
							last_modification_block: 0, /* <- We don't care about the validity
							                             * block for this value as it came from
							                             * cache */
							value,
						})
						.map(Arc::new);
				}
			}

			// For non found values, we need to query the remote storage at the first forked block
			let mut results = Vec::with_capacity(keys.len());
			for (i, value) in found_results.into_iter().enumerate() {
				let final_value = if value.is_some() {
					value
				} else {
					self.parent
						.get(self.first_forked_block_hash, keys[i])
						.await?
						.map(|value| {
							LocalSharedValue {
								last_modification_block: 0, /* <- Value came from remote layer */
								value,
							}
						})
						.map(Arc::new)
				};
				results.push(final_value);
			}
			return Ok(results);
		}

		// Case 3: Block before or at fork point - fetch and cache block if needed
		let block = self.get_block(block_number).await?;

		if let Some(block_row) = block {
			let block_hash = H256::from_slice(&block_row.hash);
			let parent_values = self.parent.get_batch(block_hash, keys).await?;
			Ok(parent_values
				.into_iter()
				.flatten()
				.map(|value| {
					LocalSharedValue {
						last_modification_block: 0, /* <- We don't care about this value as
						                             * it came
						                             * from the remote layer, */
						value,
					}
				})
				.map(Arc::new)
				.map(Some)
				.collect())
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

		let latest_block_number = self.get_latest_block_number();

		let mut modifications_lock =
			self.modifications.write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		for (key, value) in entries {
			modifications_lock.insert(
				key.to_vec(),
				value.map(|value| {
					Arc::new(LocalSharedValue {
						last_modification_block: latest_block_number,
						value: value.to_vec(),
					})
				}),
			);
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

	/// Commit modifications to the local storage tables, creating new validity entries.
	///
	/// # Returns
	/// * `Ok(())` - All modifications were successfully committed to the cache
	/// * `Err(_)` - Lock error or cache error
	///
	/// # Behavior
	/// - Only commits modifications whose `last_modification_block == latest_block_number`
	/// - For each key to commit:
	///   - If key not in local_keys: insert key, then insert value with valid_from =
	///     latest_block_number
	///   - If key exists: close current open value (set valid_until), insert new value
	/// - The modifications HashMap remains intact and available after commit
	/// - Increases the latest block number
	pub async fn commit(&mut self) -> Result<(), LocalStorageError> {
		let latest_block_number = self.get_latest_block_number();
		let new_latest_block =
			latest_block_number.checked_add(1).ok_or(LocalStorageError::Arithmetic)?;

		// Collect modifications that need to be committed (only those modified at
		// latest_block_number)
		let diff = self.diff()?;

		// Filter to only include modifications made at the current latest_block_number
		let entries_to_commit: Vec<(&[u8], &[u8])> = diff
			.iter()
			.filter_map(|(key, shared_value)| {
				shared_value.as_ref().and_then(|sv| {
					if sv.last_modification_block == latest_block_number {
						Some((key.as_slice(), sv.value.as_slice()))
					} else {
						None
					}
				})
			})
			.collect();

		// Commit
		for (key, value) in entries_to_commit {
			match self.parent.cache().get_local_key(key).await? {
				Some(LocalKeyRow { id: key_id, .. }) => {
					self.parent.cache().close_local_value(key_id, latest_block_number).await?;
					self.parent
						.cache()
						.insert_local_value(key_id, value, latest_block_number)
						.await?;
				},
				_ => {
					let key_id = self.parent.cache().insert_local_key(key).await?;
					self.parent
						.cache()
						.insert_local_value(key_id, value, latest_block_number)
						.await?;
				},
			}
		}

		self.latest_block_number = new_latest_block;

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
		let layer = create_layer(&ctx);

		// Verify empty modifications
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0, "New layer should have no modifications");
		assert_eq!(layer.first_forked_block_number, ctx.block_number);
		assert_eq!(layer.latest_block_number, ctx.block_number + 1);
	}

	// Tests for get()
	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_local_modification() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = b"test_key";
		let value = b"test_value";

		// Set a local value
		layer.set(key, Some(value)).unwrap();

		// Get should return the local value
		let result = layer.get(block, key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block,
				value: value.as_slice().to_vec()
			}))
		);

		// After a few commits, the last modification blocks remains the same
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();
		let new_block = layer.get_latest_block_number();
		let result = layer.get(new_block, key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block,
				value: value.as_slice().to_vec()
			}))
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
	async fn get_returns_none_for_deleted_prefix_if_exact_key_not_found() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = b"key";
		let prefix = b"ke";
		let value = b"value";

		layer.set(key, Some(value)).unwrap();

		layer.delete_prefix(prefix).unwrap();

		// Get should return None
		let result = layer.get(block, key).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_some_for_deleted_prefix_if_exact_key_found_after_deletion() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = b"key";
		let prefix = b"ke";
		let value = b"value";

		layer.set(key, Some(value)).unwrap();

		layer.delete_prefix(prefix).unwrap();

		// Get should return None
		let result = layer.get(block, key).await.unwrap();
		assert!(result.is_none(), "get() should return None for deleted key");

		layer.set(key, Some(value)).unwrap();
		let result = layer.get(block, key).await.unwrap();
		// the exact key is found
		assert_eq!(result.unwrap().value.as_slice(), value.as_slice());
		// even for a deleted prefix
		assert!(layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_falls_back_to_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Get without local modification - should fetch from parent
		let result = layer.get(block, &key).await.unwrap().unwrap().value.clone();
		assert_eq!(u32::decode(&mut &result[..]).unwrap(), ctx.block_number);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_overrides_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let local_value = b"local_override";

		// Get parent value first
		let parent_value = layer.get(block, &key).await.unwrap().unwrap().value.clone();
		assert_eq!(u32::decode(&mut &parent_value[..]).unwrap(), ctx.block_number);

		// Set local value
		layer.set(&key, Some(local_value)).unwrap();

		// Get should return local value, not parent
		let result = layer.get(block, &key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block,
				value: local_value.as_slice().to_vec()
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_none_for_nonexistent_key() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = b"nonexistent_key_12345";

		// Get should return None for nonexistent key
		let result = layer.get(block, key).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_retrieves_modified_value_from_past_forked_block() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let key = b"modified_key";
		let value_block_1 = b"value_at_block_1";
		let value_block_2 = b"value_at_block_2";

		// Advance one block to be fully inside the fork.
		layer.commit().await.unwrap();

		// Set and commit at block N (first_forked_block)
		layer.set(key, Some(value_block_1)).unwrap();
		layer.commit().await.unwrap();
		let block_1 = layer.get_latest_block_number() - 1; // Block where we committed

		// Set and commit at block N+1
		layer.set(key, Some(value_block_2)).unwrap();
		layer.commit().await.unwrap();
		let block_2 = layer.get_latest_block_number() - 1; // Block where we committed

		// Query at block_1 - should get value_block_1 from local_storage table
		let result_block_1 = layer.get(block_1, key).await.unwrap();
		assert_eq!(
			result_block_1,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_1,
				value: value_block_1.to_vec()
			}))
		);

		// Query at block_2 - should get value_block_2 from local_storage table
		let result_block_2 = layer.get(block_2, key).await.unwrap();
		assert_eq!(
			result_block_2,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: value_block_2.to_vec()
			}))
		);

		// Query at latest block - should get value_block_2 from modifications
		let result_latest = layer.get(layer.get_latest_block_number(), key).await.unwrap();
		assert_eq!(
			result_latest,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: value_block_2.to_vec()
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_retrieves_unmodified_value_from_remote_at_past_forked_block() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let unmodified_key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Advance a few blocks
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();
		let committed_block = layer.get_latest_block_number() - 1;

		// Query the unmodified_key at the committed block
		// Since unmodified_key was never modified, it should fall back to remote at
		// first_forked_block
		let result = layer.get(committed_block, &unmodified_key).await.unwrap();
		assert!(result.is_some(),);

		// Verify we get the same value as querying at first_forked_block directly
		let remote_value = layer.get(ctx.block_number, &unmodified_key).await.unwrap();
		assert_eq!(result, remote_value,);
	}

	// Tests for get_block (via get/get_batch for historical blocks)
	#[tokio::test(flavor = "multi_thread")]
	async fn get_historical_block() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		// Query a block that's not in cache (fork point)
		let block_number = ctx.block_number;
		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Verify block is not in cache initially
		let cached_before = ctx.remote.cache().get_block_by_number(block_number).await.unwrap();
		assert!(cached_before.is_none());

		// Get storage from historical block
		let result = layer.get(block_number, &key).await.unwrap().unwrap().value.clone();
		assert_eq!(u32::decode(&mut &result[..]).unwrap(), ctx.block_number);

		// Cached after
		let cached_before = ctx.remote.cache().get_block_by_number(block_number).await.unwrap();
		assert!(cached_before.is_some());
	}

	// Tests for set()
	#[tokio::test(flavor = "multi_thread")]
	async fn set_stores_value() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = b"key";
		let value = b"value";

		layer.set(key, Some(value)).unwrap();

		// Verify via get
		let result = layer.get(block, key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block,
				value: value.as_slice().to_vec()
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_overwrites_previous_value() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key, Some(value1)).unwrap();
		layer.set(key, Some(value2)).unwrap();

		// Should have the second value
		let result = layer.get(block, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.value.as_slice()), Some(value2.as_slice()));
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
		let block = layer.get_latest_block_number();

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set_batch(&[(key1, Some(value1)), (key2, Some(value2))]).unwrap();

		let results = layer.get_batch(block, &[key1, key2]).await.unwrap();
		assert_eq!(results.len(), 2);
		assert_eq!(results[0].as_ref().map(|v| v.value.as_slice()), Some(value1.as_slice()));
		assert_eq!(results[1].as_ref().map(|v| v.value.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_none_for_deleted_prefix() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key1 = b"key1";
		let key2 = b"key2";

		layer.set_batch(&[(key1, Some(b"val")), (key2, Some(b"val"))]).unwrap();
		layer.delete_prefix(key2).unwrap();

		let results = layer.get_batch(block, &[key1, key2]).await.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_falls_back_to_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

		let results = layer.get_batch(block, &[key1.as_slice(), key2.as_slice()]).await.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_some());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_local_overrides_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
		let local_value = b"local_override";

		// Set one key locally
		layer.set(&key1, Some(local_value)).unwrap();

		let results = layer.get_batch(block, &[key1.as_slice(), key2.as_slice()]).await.unwrap();
		assert_eq!(results[0].as_ref().map(|v| v.value.as_slice()), Some(local_value.as_slice()));
		assert!(results[1].is_some());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_mixed_sources() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let local_key = b"local_key";
		let remote_key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let deleted_key = b"deleted_key";
		let nonexistent_key = b"nonexistent_key";

		layer.set(local_key, Some(b"local_value")).unwrap();
		layer.set(deleted_key, None).unwrap();

		let results = layer
			.get_batch(block, &[local_key, remote_key.as_slice(), deleted_key, nonexistent_key])
			.await
			.unwrap();

		assert_eq!(results.len(), 4);
		assert_eq!(
			results[0].as_ref().map(|v| v.value.as_slice()),
			Some(b"local_value".as_slice())
		);
		assert_eq!(
			u32::decode(&mut &results[1].as_ref().unwrap().value[..]).unwrap(),
			ctx.block_number
		); // from parent
		assert!(results[2].is_none()); // deleted
		assert!(results[3].is_none()); // nonexistent
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_maintains_order() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

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
		let results = layer.get_batch(block, &[key3, key1, key2]).await.unwrap();
		assert_eq!(results[0].as_ref().map(|v| v.value.as_slice()), Some(value3.as_slice()));
		assert_eq!(results[1].as_ref().map(|v| v.value.as_slice()), Some(value1.as_slice()));
		assert_eq!(results[2].as_ref().map(|v| v.value.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_retrieves_modified_value_from_past_forked_block() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let key1 = b"modified_key1";
		let key2 = b"modified_key2";
		let value1_block_1 = b"value1_at_block_1";
		let value2_block_1 = b"value2_at_block_1";
		let value1_block_2 = b"value1_at_block_2";
		let value2_block_2 = b"value2_at_block_2";

		// Advance one block to be fully inside the fork
		layer.commit().await.unwrap();

		// Set and commit at block N
		layer
			.set_batch(&[(key1, Some(value1_block_1)), (key2, Some(value2_block_1))])
			.unwrap();
		layer.commit().await.unwrap();
		let block_1 = layer.get_latest_block_number() - 1;

		// Set and commit at block N+1
		layer
			.set_batch(&[(key1, Some(value1_block_2)), (key2, Some(value2_block_2))])
			.unwrap();
		layer.commit().await.unwrap();
		let block_2 = layer.get_latest_block_number() - 1;

		// Query at block_1 - should get values from local_storage table
		let results_block_1 = layer.get_batch(block_1, &[key1, key2]).await.unwrap();
		assert_eq!(
			results_block_1[0],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_1,
				value: value1_block_1.to_vec()
			}))
		);
		assert_eq!(
			results_block_1[1],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_1,
				value: value2_block_1.to_vec()
			}))
		);

		// Query at block_2 - should get values from local_storage table
		let results_block_2 = layer.get_batch(block_2, &[key1, key2]).await.unwrap();
		assert_eq!(
			results_block_2[0],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: value1_block_2.to_vec()
			}))
		);
		assert_eq!(
			results_block_2[1],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: value2_block_2.to_vec()
			}))
		);

		// Query at latest block - should get values from modifications
		let results_latest =
			layer.get_batch(layer.get_latest_block_number(), &[key1, key2]).await.unwrap();
		assert_eq!(
			results_latest[0],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: value1_block_2.to_vec()
			}))
		);
		assert_eq!(
			results_latest[1],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: value2_block_2.to_vec()
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_retrieves_unmodified_value_from_remote_at_past_forked_block() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let unmodified_key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let unmodified_key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

		// Advance a few blocks
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();
		let committed_block = layer.get_latest_block_number() - 1;

		// Query the unmodified keys at the committed block
		// Since they were never modified, they should fall back to remote at first_forked_block
		let results = layer
			.get_batch(committed_block, &[unmodified_key1.as_slice(), unmodified_key2.as_slice()])
			.await
			.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_some());

		// Verify we get the same values as querying at first_forked_block directly
		let remote_values = layer
			.get_batch(ctx.block_number, &[unmodified_key1.as_slice(), unmodified_key2.as_slice()])
			.await
			.unwrap();
		assert_eq!(results[0], remote_values[0]);
		assert_eq!(results[1], remote_values[1]);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_historical_block() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		// Wait for some blocks to be finalized
		std::thread::sleep(Duration::from_secs(30));

		// Query a block that's not in cache
		let block_number = ctx.block_number;
		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

		// Get storage from historical block
		let results = layer
			.get_batch(block_number, &[key1.as_slice(), key2.as_slice()])
			.await
			.unwrap();
		assert_eq!(results.len(), 2);
		assert_eq!(
			u32::decode(&mut &results[0].as_ref().unwrap().value[..]).unwrap(),
			block_number
		);
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

		// Advance some blocks
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();

		let latest_block_1 = layer.get_latest_block_number();

		let key1 = b"local_key";
		let key2 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Set a local modification
		layer.set(key1, Some(b"local_value")).unwrap();

		// Get from latest block (should hit modifications)
		let results1 = layer.get(latest_block_1, key1).await.unwrap();
		assert_eq!(results1.as_ref().map(|v| v.value.as_slice()), Some(b"local_value".as_slice()));

		// Get from historical block (should fetch and cache block)
		let historical_block = ctx.block_number;
		let results2 = layer
			.get(historical_block, key2.as_slice())
			.await
			.unwrap()
			.unwrap()
			.value
			.clone();
		assert_eq!(u32::decode(&mut &results2[..]).unwrap(), historical_block);

		// Commit block modifications
		layer.commit().await.unwrap();

		let latest_block_2 = layer.get_latest_block_number();

		layer.set(key1, Some(b"local_value_2")).unwrap();

		let result_previous_block = layer.get(latest_block_1, key1).await.unwrap().unwrap();
		let result_latest_block = layer.get(latest_block_2, key1).await.unwrap().unwrap();

		assert_eq!(
			*result_previous_block,
			LocalSharedValue {
				last_modification_block: latest_block_1,
				value: b"local_value".to_vec()
			}
		);
		assert_eq!(
			*result_latest_block,
			LocalSharedValue {
				last_modification_block: latest_block_2,
				value: b"local_value_2".to_vec()
			}
		);
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
		let block = layer.get_latest_block_number();

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";

		layer.set_batch(&[(key1, Some(value1)), (key2, None)]).unwrap();

		let results = layer.get_batch(block, &[key1, key2]).await.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_overwrites_previous_values() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key, Some(value1)).unwrap();
		layer.set_batch(&[(key, Some(value2))]).unwrap();

		let result = layer.get(block, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.value.as_slice()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_duplicate_keys_last_wins() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		// Set same key twice in one batch - last should win
		layer.set_batch(&[(key, Some(value1)), (key, Some(value2))]).unwrap();

		let result = layer.get(block, key).await.unwrap();
		assert_eq!(result.as_ref().map(|v| v.value.as_slice()), Some(value2.as_slice()));
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
		let block = layer.get_latest_block_number();

		let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();
		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Verify key exists in parent
		let before = layer.get(block, &key).await.unwrap();
		assert!(before.is_some());

		// Delete prefix
		layer.delete_prefix(&prefix).unwrap();

		// Should return None now
		let after = layer.get(block, &key).await.unwrap();
		assert!(after.is_none());
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
			diff.iter().any(|(k, v)| k == key1 &&
				v.as_ref().map(|v| v.value.as_slice()) == Some(value1.as_slice()))
		);
		assert!(
			diff.iter().any(|(k, v)| k == key2 &&
				v.as_ref().map(|v| v.value.as_slice()) == Some(value2.as_slice()))
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

	// Tests for commit()
	#[tokio::test(flavor = "multi_thread")]
	async fn commit_writes_to_cache() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let block = layer.get_latest_block_number();

		let key1 = b"commit_key1";
		let key2 = b"commit_key2";
		let value1 = b"commit_value1";
		let value2 = b"commit_value2";

		// Set local modifications
		layer.set(key1, Some(value1)).unwrap();
		layer.set(key2, Some(value2)).unwrap();

		// Verify not in cache yet
		assert!(ctx.remote.cache().get_local_storage(block, key1).await.unwrap().is_none());
		assert!(ctx.remote.cache().get_local_storage(block, key2).await.unwrap().is_none());

		// Commit
		layer.commit().await.unwrap();

		// Verify now in cache at the block_number it was committed to
		let cached1 = ctx.remote.cache().get_local_storage(block, key1).await.unwrap();
		let cached2 = ctx.remote.cache().get_local_storage(block, key2).await.unwrap();

		assert_eq!(cached1, Some(Some(value1.to_vec())));
		assert_eq!(cached2, Some(Some(value2.to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_preserves_modifications() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);

		let block = layer.get_latest_block_number();

		let key = b"preserve_key";
		let value = b"preserve_value";

		// Set and commit
		layer.set(key, Some(value)).unwrap();
		layer.commit().await.unwrap();

		// Modifications should still be in local layer
		let local_result = layer.get(block + 1, key).await.unwrap();
		assert_eq!(local_result.as_ref().map(|v| v.value.as_slice()), Some(value.as_slice()));

		// Should also be in diff
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 1);
		assert_eq!(diff[0].0, key);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_with_deletions() {
		let ctx = create_test_context().await;
		let mut layer = create_layer(&ctx);
		let block = layer.get_latest_block_number();

		let key1 = b"delete_key1";
		let key2 = b"delete_key2";
		let value = b"value";

		// Set one value and mark another as deleted
		layer.set(key1, Some(value)).unwrap();
		layer.set(key2, None).unwrap();

		// Commit
		layer.commit().await.unwrap();

		// Both should be in cache
		let cached1 = ctx.remote.cache().get_local_storage(block, key1).await.unwrap();
		let cached2 = ctx.remote.cache().get_local_storage(block, key2).await.unwrap();

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
		let block = layer.get_latest_block_number();

		let key = b"multi_block_key";
		let value = b"multi_block_value";

		// Set local modification
		layer.set(key, Some(value)).unwrap();

		// Commit multiple times - each commit increments the block number
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();

		// Both block numbers should have the value in cache
		let cached1 = ctx.remote.cache().get_local_storage(block, key).await.unwrap();
		let cached2 = ctx.remote.cache().get_local_storage(block + 1, key).await.unwrap();

		assert_eq!(cached1, Some(Some(value.to_vec())));
		assert_eq!(cached2, Some(Some(value.to_vec())));
	}

	// Tests for next_key()
	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_returns_next_key_from_parent() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

		// Get the first key in the System pallet (starting from empty key)
		let first_key = layer.next_key(&prefix, &[]).await.unwrap();
		assert!(first_key.is_some(), "System pallet should have at least one key");

		let first_key = first_key.unwrap();
		assert!(first_key.starts_with(&prefix), "Returned key should start with the prefix");

		// Get the next key after the first one
		let second_key = layer.next_key(&prefix, &first_key).await.unwrap();
		assert!(second_key.is_some(), "System pallet should have more than one key");

		let second_key = second_key.unwrap();
		assert!(second_key.starts_with(&prefix), "Second key should also start with the prefix");
		assert!(second_key > first_key, "Second key should be greater than first key");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_returns_none_when_no_more_keys() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		// Use a prefix that doesn't exist
		let nonexistent_prefix = b"nonexistent_prefix_12345";

		let result = layer.next_key(nonexistent_prefix, &[]).await.unwrap();
		assert!(result.is_none(), "Should return None for nonexistent prefix");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_skips_deleted_prefix() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

		// Get the first two keys
		let first_key = layer.next_key(&prefix, &[]).await.unwrap().unwrap();
		let second_key = layer.next_key(&prefix, &first_key).await.unwrap().unwrap();

		// Delete the prefix that matches the first key (delete the first key specifically)
		layer.delete_prefix(&first_key).unwrap();

		// Now when we query from empty, we should skip the first key and get the second
		let result = layer.next_key(&prefix, &[]).await.unwrap();
		assert!(result.is_some(), "Should find a key after skipping deleted one");
		assert_eq!(
			result.unwrap(),
			second_key,
			"Should return second key after skipping deleted first key"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_skips_multiple_deleted_keys() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

		// Get the first three keys
		let first_key = layer.next_key(&prefix, &[]).await.unwrap().unwrap();
		let second_key = layer.next_key(&prefix, &first_key).await.unwrap().unwrap();
		let third_key = layer.next_key(&prefix, &second_key).await.unwrap().unwrap();

		// Delete the first two keys
		layer.delete_prefix(&first_key).unwrap();
		layer.delete_prefix(&second_key).unwrap();

		// Query from empty should skip both and return the third
		let result = layer.next_key(&prefix, &[]).await.unwrap();
		assert!(result.is_some(), "Should find a key after skipping deleted ones");
		assert_eq!(
			result.unwrap(),
			third_key,
			"Should return third key after skipping first two deleted keys"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_returns_none_when_all_remaining_deleted() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

		// Delete the entire System pallet prefix
		layer.delete_prefix(&prefix).unwrap();

		// All keys under System pallet should be skipped
		let result = layer.next_key(&prefix, &[]).await.unwrap();
		assert!(result.is_none(), "Should return None when all keys match deleted prefix");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_with_empty_prefix() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		// Empty prefix should match all keys
		let result = layer.next_key(&[], &[]).await.unwrap();
		assert!(result.is_some(), "Empty prefix should return some key from storage");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_with_nonexistent_prefix() {
		let ctx = create_test_context().await;
		let layer = create_layer(&ctx);

		let nonexistent_prefix = b"this_prefix_definitely_does_not_exist_xyz";

		let result = layer.next_key(nonexistent_prefix, &[]).await.unwrap();
		assert!(result.is_none(), "Nonexistent prefix should return None");
	}
}
