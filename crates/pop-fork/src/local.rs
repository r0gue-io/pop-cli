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
use scale::Decode;
use std::{
	collections::{BTreeMap, HashMap},
	sync::{Arc, RwLock},
};
use subxt::{Metadata, config::substrate::H256};

const ONE_BLOCK: u32 = 1;

/// A value that can be shared accross different local storage layer instances
#[derive(Debug, PartialEq)]
pub struct LocalSharedValue {
	last_modification_block: u32,
	/// The actual value
	pub value: Option<Vec<u8>>,
}

static EMPTY_MEMORY: [u8; 0] = [];
impl AsRef<[u8]> for LocalSharedValue {
	/// AsRef implementation to get the value bytes of this local shared value
	fn as_ref(&self) -> &[u8] {
		match &self.value {
			Some(value) => value.as_ref(),
			None => &EMPTY_MEMORY,
		}
	}
}

type SharedValue = Arc<LocalSharedValue>;
type Modifications = HashMap<Vec<u8>, Option<SharedValue>>;
type DeletedPrefixes = Vec<Vec<u8>>;
type DiffLocalStorage = Vec<(Vec<u8>, Option<SharedValue>)>;
/// Maps block number (when metadata became valid) to metadata.
/// Used to track metadata versions across runtime upgrades.
type MetadataVersions = BTreeMap<u32, Arc<Metadata>>;

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
	current_block_number: u32,
	modifications: Arc<RwLock<Modifications>>,
	deleted_prefixes: Arc<RwLock<DeletedPrefixes>>,
	/// Metadata versions indexed by the block number when they became valid.
	/// Enables looking up the correct metadata for any block in the fork.
	metadata_versions: Arc<RwLock<MetadataVersions>>,
}

impl LocalStorageLayer {
	/// Create a new local storage layer.
	///
	/// # Arguments
	/// * `parent` - The remote storage layer to use as the base state
	/// * `first_forked_block_number` - The initial block number where the fork started
	/// * `first_forked_block_hash` - The hash of the first forked block
	/// * `metadata` - The runtime metadata at the fork point
	///
	/// # Returns
	/// A new `LocalStorageLayer` with no modifications, with `current_block_number` set to
	/// `first_forked_block_number + 1` (the block being built).
	pub fn new(
		parent: RemoteStorageLayer,
		first_forked_block_number: u32,
		first_forked_block_hash: H256,
		metadata: Metadata,
	) -> Self {
		let mut metadata_versions = BTreeMap::new();
		metadata_versions.insert(first_forked_block_number, Arc::new(metadata));

		Self {
			parent,
			first_forked_block_hash,
			first_forked_block_number,
			current_block_number: first_forked_block_number + 1, /* current_block_number is the
			                                                      * one to be produced. */
			modifications: Arc::new(RwLock::new(HashMap::new())),
			deleted_prefixes: Arc::new(RwLock::new(Vec::new())),
			metadata_versions: Arc::new(RwLock::new(metadata_versions)),
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

	/// Get the current block number.
	pub fn get_current_block_number(&self) -> u32 {
		self.current_block_number
	}

	/// Get a reference to the underlying storage cache.
	///
	/// This provides access to the cache for operations like clearing local storage.
	pub fn cache(&self) -> &crate::StorageCache {
		self.parent.cache()
	}

	/// Get the underlying remote storage layer.
	///
	/// This provides access to the remote layer for operations that need to
	/// fetch data directly from the remote chain (e.g., block headers, bodies).
	/// The remote layer maintains a persistent connection to the RPC endpoint.
	pub fn remote(&self) -> &crate::RemoteStorageLayer {
		&self.parent
	}

	/// Get the hash of the first forked block.
	///
	/// This is the block hash at which the fork was created, used for querying
	/// storage keys on the remote chain.
	pub fn fork_block_hash(&self) -> H256 {
		self.first_forked_block_hash
	}

	/// Get the metadata valid at a specific block number.
	///
	/// For blocks at or after the fork point, returns metadata from the local version tree.
	/// For blocks before the fork point, fetches metadata from the remote node.
	///
	/// # Arguments
	/// * `block_number` - The block number to get metadata for
	///
	/// # Returns
	/// * `Ok(Arc<Metadata>)` - The metadata valid at the given block
	/// * `Err(_)` - Lock error, RPC error, or metadata decode error
	pub async fn metadata_at(&self, block_number: u32) -> Result<Arc<Metadata>, LocalStorageError> {
		// For blocks before the fork point, fetch from remote
		if block_number < self.first_forked_block_number {
			return self.fetch_remote_metadata(block_number).await;
		}

		// For blocks at or after fork point, use local version tree
		let versions = self
			.metadata_versions
			.read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		versions
			.range(..=block_number)
			.next_back()
			.map(|(_, metadata)| Arc::clone(metadata))
			.ok_or_else(|| {
				LocalStorageError::MetadataNotFound(format!(
					"No metadata found for block {}",
					block_number
				))
			})
	}

	/// Fetch metadata from the remote node, usually used for a pre-fork block.
	async fn fetch_remote_metadata(
		&self,
		block_number: u32,
	) -> Result<Arc<Metadata>, LocalStorageError> {
		// Get block hash for this block number
		let block_hash = self.parent.rpc().block_hash_at(block_number).await?.ok_or_else(|| {
			LocalStorageError::MetadataNotFound(format!(
				"Block {} not found on remote node",
				block_number
			))
		})?;

		// Fetch metadata bytes from remote
		let metadata_bytes = self.parent.rpc().metadata(block_hash).await?;

		// Decode metadata
		let metadata = Metadata::decode(&mut metadata_bytes.as_slice()).map_err(|e| {
			LocalStorageError::MetadataNotFound(format!("Failed to decode remote metadata: {}", e))
		})?;

		Ok(Arc::new(metadata))
	}

	/// Register a new metadata version starting at the given block number.
	///
	/// This should be called when a runtime upgrade occurs (`:code` storage key changes)
	/// to record that a new metadata version is now active.
	///
	/// # Arguments
	/// * `block_number` - The block number where this metadata becomes valid
	/// * `metadata` - The new runtime metadata
	///
	/// # Returns
	/// * `Ok(())` - Metadata version registered successfully
	/// * `Err(_)` - Lock error
	pub fn register_metadata_version(
		&self,
		block_number: u32,
		metadata: Metadata,
	) -> Result<(), LocalStorageError> {
		let mut versions = self
			.metadata_versions
			.write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		versions.insert(block_number, Arc::new(metadata));
		Ok(())
	}

	/// Check if the `:code` storage key was modified at the specified block.
	///
	/// This is used to detect runtime upgrades. When a runtime upgrade occurs in block X,
	/// the new runtime is used starting from block X+1. So when building X+1, we check
	/// if code changed in X (the parent) to determine if we're now using a new runtime.
	///
	/// # Arguments
	/// * `block_number` - The block number to check for code modifications
	///
	/// # Returns
	/// * `Ok(true)` - The `:code` key was modified at the specified block
	/// * `Ok(false)` - The `:code` key was not modified at the specified block
	/// * `Err(_)` - Lock error
	pub fn has_code_changed_at(&self, block_number: u32) -> Result<bool, LocalStorageError> {
		let modifications =
			self.modifications.read().map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		// The well-known `:code` storage key
		let code_key = sp_core::storage::well_known_keys::CODE;

		// Check if :code was modified at the specified block
		Ok(modifications.get(code_key).is_some_and(|value| {
			value.as_ref().is_some_and(|v| v.last_modification_block == block_number)
		}))
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
	) -> Result<Option<Option<SharedValue>>, LocalStorageError> {
		let latest_block_number = self.get_current_block_number();
		let modifications_lock =
			self.modifications.read().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let deleted_prefixes_lock = self
			.deleted_prefixes
			.read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		match modifications_lock.get(key) {
			local_modification @ Some(Some(shared_value))
				if latest_block_number == block_number ||
					shared_value.last_modification_block < block_number =>
				Ok(local_modification.cloned()), /* <- Cheap clone as it's Option<Option<Arc<_>>> */
			None if deleted_prefixes_lock
				.iter()
				.any(|prefix| key.starts_with(prefix.as_slice())) =>
				Ok(Some(None)),
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
		let latest_block_number = self.get_current_block_number();

		// First check if the key has a local modification
		if let local_modification @ Ok(Some(_)) = self.get_local_modification(key, block_number) {
			return local_modification.map(|local_modification| local_modification.flatten());
		}

		// Case 1: Query for latest block - check modifications, then remote at first_forked_block
		if block_number == latest_block_number {
			// Not in modifications, query remote at first_forked_block
			return Ok(self
				.parent
				.get(self.first_forked_block_hash, key)
				.await?
				.map(|value| LocalSharedValue {
					last_modification_block: 0, /* <- We don't care about the validity block for
					                             * this value as it came from the remote layer */
					value: Some(value),
				})
				.map(Arc::new));
		}

		// Case 2: Historical block after fork such that the local modification is still valid -
		// check the local modifications map, otherwise fallback to cache and finally remote layer.
		if block_number > self.first_forked_block_number && block_number < latest_block_number {
			// Try to get value from local_values table using validity ranges
			// get_local_value_at_block returns Option<Option<Vec<u8>>>:
			// - None = no row found
			// - Some(None) = row found but value is NULL (deleted)
			// - Some(Some(value)) = row found with data
			let value = if let Some(local_value) =
				self.parent.cache().get_local_value_at_block(key, block_number).await?
			{
				local_value
			}
			// Not found in local storage, try remote at first_forked_block
			else if let Some(remote_value) =
				self.parent.get(self.first_forked_block_hash, key).await?
			{
				Some(remote_value)
			} else {
				return Ok(None);
			};

			return Ok(Some(Arc::new(LocalSharedValue {
				last_modification_block: 0, /* <- Value came from remote or cache layer */
				value,
			})));
		}

		// Case 3: Block before or at fork point
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
					value: Some(value),
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

	/// Enumerate all keys matching a prefix, merging remote and local state.
	///
	/// This method combines keys from the remote layer (at the fork point) with
	/// locally modified keys, producing a sorted, deduplicated list of keys that
	/// exist at the specified fork-local block.
	///
	/// For the latest block, uses the in-memory `modifications` and
	/// `deleted_prefixes` snapshots. For historical fork-local blocks, queries
	/// the persisted local values in the cache to reconstruct the key set that
	/// existed at that block.
	///
	/// Keys that were deleted locally (either individually via `set(key, None)`
	/// or via `delete_prefix`) are excluded.
	pub async fn keys_by_prefix(
		&self,
		prefix: &[u8],
		block_number: u32,
	) -> Result<Vec<Vec<u8>>, LocalStorageError> {
		// 1. Get remote keys at the fork point.
		let remote_keys = self.parent.get_keys(self.first_forked_block_hash, prefix).await?;

		let latest_block_number = self.get_current_block_number();

		if block_number >= latest_block_number {
			// Latest block: use in-memory modifications (fast path).
			self.merge_keys_with_in_memory(remote_keys, prefix)
		} else {
			// Historical fork-local block: query persisted local values from cache.
			self.merge_keys_with_cache(remote_keys, prefix, block_number).await
		}
	}

	/// Merge remote keys with in-memory modifications for the latest block.
	fn merge_keys_with_in_memory(
		&self,
		remote_keys: Vec<Vec<u8>>,
		prefix: &[u8],
	) -> Result<Vec<Vec<u8>>, LocalStorageError> {
		let modifications = self
			.modifications
			.read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?
			.clone();
		let deleted_prefixes = self
			.deleted_prefixes
			.read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?
			.clone();

		let is_deleted = |key: &[u8]| -> bool {
			deleted_prefixes.iter().any(|dp| key.starts_with(dp.as_slice()))
		};

		let is_locally_deleted = |key: &[u8]| -> bool {
			modifications
				.get::<[u8]>(key)
				.and_then(|sv| sv.as_ref())
				.is_some_and(|sv| sv.value.is_none())
		};

		let mut merged: std::collections::BTreeSet<Vec<u8>> = remote_keys
			.into_iter()
			.filter(|k| !is_deleted(k) && !is_locally_deleted(k))
			.collect();

		for (key, maybe_sv) in modifications.iter() {
			if key.starts_with(prefix) && maybe_sv.as_ref().is_some_and(|sv| sv.value.is_some()) {
				merged.insert(key.clone());
			}
		}

		Ok(merged.into_iter().collect())
	}

	/// Merge remote keys with persisted cache data for a historical fork-local block.
	async fn merge_keys_with_cache(
		&self,
		remote_keys: Vec<Vec<u8>>,
		prefix: &[u8],
		block_number: u32,
	) -> Result<Vec<Vec<u8>>, LocalStorageError> {
		let cache = self.parent.cache();

		// Get keys that had non-NULL values at this block.
		let local_live_keys = cache.get_local_keys_at_block(prefix, block_number).await?;

		// Get keys that were explicitly deleted at this block.
		let local_deleted_keys =
			cache.get_local_deleted_keys_at_block(prefix, block_number).await?;

		let deleted_set: std::collections::HashSet<Vec<u8>> =
			local_deleted_keys.into_iter().collect();

		// Start with remote keys, excluding those deleted locally at this block.
		let mut merged: std::collections::BTreeSet<Vec<u8>> =
			remote_keys.into_iter().filter(|k| !deleted_set.contains(k)).collect();

		// Add locally-live keys.
		merged.extend(local_live_keys);

		Ok(merged.into_iter().collect())
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

		let latest_block_number = self.get_current_block_number();

		modifications_lock.insert(
			key.to_vec(),
			Some(Arc::new({
				LocalSharedValue {
					last_modification_block: latest_block_number,
					value: value.map(|value| value.to_vec()),
				}
			})),
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

		let latest_block_number = self.get_current_block_number();
		let mut results: Vec<Option<SharedValue>> = Vec::with_capacity(keys.len());
		let mut non_local_keys: Vec<&[u8]> = Vec::new();
		let mut non_local_indices: Vec<usize> = Vec::new();

		for (i, key) in keys.iter().enumerate() {
			match self.get_local_modification(key, block_number)? {
				Some(local_modification) => results.push(local_modification),
				_ => {
					results.push(None);
					non_local_keys.push(*key);
					non_local_indices.push(i)
				},
			}
		}

		// Case 1: Query for latest block - Complete non local keys with the remote layer
		if block_number == latest_block_number {
			if !non_local_keys.is_empty() {
				let parent_values =
					self.parent.get_batch(self.first_forked_block_hash, &non_local_keys).await?;
				for (i, parent_value) in parent_values.into_iter().enumerate() {
					let result_idx = non_local_indices[i];
					results[result_idx] = parent_value
						.map(|value| LocalSharedValue {
							last_modification_block: 0, /* <- We don't care about the validity
							                             * block for this value as it came from
							                             * remote layer */
							value: Some(value),
						})
						.map(Arc::new);
				}
			}

			return Ok(results);
		}

		// Case 2: Historical block after fork -
		// local_values table (using validity) for non local keys. Remote query for non found keys
		if block_number > self.first_forked_block_number && block_number < latest_block_number {
			if !non_local_keys.is_empty() {
				// Use validity-based query to get values from local_values table
				// get_local_values_at_block_batch returns Vec<Option<Option<Vec<u8>>>>:
				// - None = no row found
				// - Some(None) = row found but value is NULL (deleted)
				// - Some(Some(value)) = row found with data
				let cached_values = self
					.parent
					.cache()
					.get_local_values_at_block_batch(&non_local_keys, block_number)
					.await?;
				for (i, cache_value) in cached_values.into_iter().enumerate() {
					let result_idx = non_local_indices[i];
					// cache_value is Option<Option<Vec<u8>>>, map it to Option<SharedValue>
					results[result_idx] = cache_value.map(|value| {
						Arc::new(LocalSharedValue {
							last_modification_block: 0, /* <- We don't care about the validity
							                             * block for this value as it came from
							                             * cache */
							value,
						})
					});
				}
			}

			// For non found values, we need to query the remote storage at the first forked block
			let mut final_results = Vec::with_capacity(keys.len());
			for (i, value) in results.into_iter().enumerate() {
				let final_value = if value.is_some() {
					value
				} else {
					self.parent
						.get(self.first_forked_block_hash, keys[i])
						.await?
						.map(|value| {
							LocalSharedValue {
								last_modification_block: 0, /* <- Value came from remote layer */
								value: Some(value),
							}
						})
						.map(Arc::new)
				};
				final_results.push(final_value);
			}
			return Ok(final_results);
		}

		// Case 3: Block before or at fork point - fetch and cache block if needed
		let block = self.get_block(block_number).await?;

		if let Some(block_row) = block {
			let block_hash = H256::from_slice(&block_row.hash);
			let parent_values = self.parent.get_batch(block_hash, keys).await?;
			Ok(parent_values
				.into_iter()
				.map(|value| {
					value.map(|value| {
						Arc::new(LocalSharedValue {
							last_modification_block: 0, /* <- We don't care about this value as
							                             * it came
							                             * from the remote layer, */
							value: Some(value),
						})
					})
				})
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

		let latest_block_number = self.get_current_block_number();

		let mut modifications_lock =
			self.modifications.write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		for (key, value) in entries {
			modifications_lock.insert(
				key.to_vec(),
				Some(Arc::new(LocalSharedValue {
					last_modification_block: latest_block_number,
					value: value.map(|value| value.to_vec()),
				})),
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
		let current_block_number = self.get_current_block_number();
		let new_latest_block = current_block_number
			.checked_add(ONE_BLOCK)
			.ok_or(LocalStorageError::Arithmetic)?;

		// Collect modifications that need to be committed (only those modified at
		// latest_block_number)
		let diff = self.diff()?;

		// Filter to only include modifications made at the current latest_block_number
		// entries_to_commit contains (key, Option<value>) where None means deletion
		let entries_to_commit: Vec<(&[u8], Option<&[u8]>)> = diff
			.iter()
			.filter_map(|(key, shared_value)| {
				shared_value.as_ref().and_then(|sv| {
					if sv.last_modification_block == current_block_number {
						Some((key.as_slice(), sv.value.as_deref()))
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
					self.parent.cache().close_local_value(key_id, current_block_number).await?;
					self.parent
						.cache()
						.insert_local_value(key_id, value, current_block_number)
						.await?;
				},
				_ => {
					let key_id = self.parent.cache().insert_local_key(key).await?;
					self.parent
						.cache()
						.insert_local_value(key_id, value, current_block_number)
						.await?;
				},
			}
		}

		self.current_block_number = new_latest_block;

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
	use crate::testing::{
		TestContext,
		constants::{SYSTEM_NUMBER_KEY, SYSTEM_PALLET_PREFIX, SYSTEM_PARENT_HASH_KEY},
	};
	use std::time::Duration;
	use subxt::ext::codec::Decode;

	/// Helper to create a LocalStorageLayer with proper block hash and number
	fn create_layer(ctx: &TestContext) -> LocalStorageLayer {
		LocalStorageLayer::new(
			ctx.remote().clone(),
			ctx.block_number(),
			ctx.block_hash(),
			ctx.metadata().clone(),
		)
	}

	// Tests for new()
	#[tokio::test(flavor = "multi_thread")]
	async fn new_creates_empty_layer() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Verify empty modifications
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0, "New layer should have no modifications");
		assert_eq!(layer.first_forked_block_number, ctx.block_number());
		assert_eq!(layer.current_block_number, ctx.block_number() + 1);
	}

	// Tests for get()
	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_local_modification() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

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
				value: Some(value.as_slice().to_vec())
			}))
		);

		// After a few commits, the last modification blocks remains the same
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();
		let new_block = layer.get_current_block_number();
		let result = layer.get(new_block, key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block,
				value: Some(value.as_slice().to_vec())
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_non_existent_block_returns_none() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Query a block that doesn't exist
		let non_existent_block = u32::MAX;
		let key = b"some_key";

		let result = layer.get(non_existent_block, key).await.unwrap();
		assert!(result.is_none(), "Non-existent block should return None");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_none_for_deleted_prefix_if_exact_key_not_found() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

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
		assert_eq!(result.unwrap().value.as_deref().unwrap(), value.as_slice());
		// even for a deleted prefix
		assert!(layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_falls_back_to_parent() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Get without local modification - should fetch from parent
		let result = layer.get(block, &key).await.unwrap().unwrap().value.clone().unwrap();
		assert_eq!(u32::decode(&mut &result[..]).unwrap(), ctx.block_number());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_overrides_parent() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let local_value = b"local_override";

		// Get parent value first
		let parent_value = layer.get(block, &key).await.unwrap().unwrap().value.clone().unwrap();
		assert_eq!(u32::decode(&mut &parent_value[..]).unwrap(), ctx.block_number());

		// Set local value
		layer.set(&key, Some(local_value)).unwrap();

		// Get should return local value, not parent
		let result = layer.get(block, &key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block,
				value: Some(local_value.as_slice().to_vec())
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_returns_none_for_nonexistent_key() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key = b"nonexistent_key_12345";

		// Get should return None for nonexistent key
		let result = layer.get(block, key).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_retrieves_modified_value_from_fork_history() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		let key = b"modified_key";
		let value_block_1 = b"value_at_block_1";
		let value_block_2 = b"value_at_block_2";

		// Advance one block to be fully inside the fork.
		layer.commit().await.unwrap();

		// Set and commit at block N (first_forked_block)
		layer.set(key, Some(value_block_1)).unwrap();
		layer.commit().await.unwrap();
		let block_1 = layer.get_current_block_number() - 1; // Block where we committed

		// Set and commit at block N+1
		layer.set(key, Some(value_block_2)).unwrap();
		layer.commit().await.unwrap();
		let block_2 = layer.get_current_block_number() - 1; // Block where we committed

		// Query at block_1 - should get value_block_1 from local_storage table
		let result_block_1 = layer.get(block_1, key).await.unwrap();
		assert_eq!(
			result_block_1,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: 0, // <- Comes from cache, not hot key so set to 0
				value: Some(value_block_1.to_vec())
			}))
		);

		// Query at block_2 - should get value_block_2 from local_storage table
		let result_block_2 = layer.get(block_2, key).await.unwrap();
		assert_eq!(
			result_block_2,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: 0, // <- Comes from cache, not hot key so set to 0
				value: Some(value_block_2.to_vec())
			}))
		);

		// Query at latest block - should get value_block_2 from modifications
		let result_latest = layer.get(layer.get_current_block_number(), key).await.unwrap();
		assert_eq!(
			result_latest,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: Some(value_block_2.to_vec())
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_retrieves_unmodified_value_from_remote_at_past_forked_block() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		let unmodified_key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Advance a few blocks
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();
		let committed_block = layer.get_current_block_number() - 1;

		// Query the unmodified_key at the committed block
		// Since unmodified_key was never modified, it should fall back to remote at
		// first_forked_block
		let result = layer.get(committed_block, &unmodified_key).await.unwrap();
		assert!(result.is_some(),);

		// Verify we get the same value as querying at first_forked_block directly
		let remote_value = layer.get(ctx.block_number(), &unmodified_key).await.unwrap();
		assert_eq!(result, remote_value,);
	}

	// Tests for get_block (via get/get_batch for historical blocks)
	#[tokio::test(flavor = "multi_thread")]
	async fn get_historical_block() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Query a block that's not in cache (fork point)
		let block_number = ctx.block_number();
		let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Verify block is not in cache initially
		let cached_before = ctx.remote().cache().get_block_by_number(block_number).await.unwrap();
		assert!(cached_before.is_none());

		// Get storage from historical block
		let result = layer.get(block_number, &key).await.unwrap().unwrap().value.clone().unwrap();
		assert_eq!(u32::decode(&mut &result[..]).unwrap(), ctx.block_number());

		// Cached after
		let cached_before = ctx.remote().cache().get_block_by_number(block_number).await.unwrap();
		assert!(cached_before.is_some());
	}

	// Tests for set()
	#[tokio::test(flavor = "multi_thread")]
	async fn set_stores_value() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key = b"key";
		let value = b"value";

		layer.set(key, Some(value)).unwrap();

		// Verify via get
		let result = layer.get(block, key).await.unwrap();
		assert_eq!(
			result,
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block,
				value: Some(value.as_slice().to_vec())
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_overwrites_previous_value() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key, Some(value1)).unwrap();
		layer.set(key, Some(value2)).unwrap();

		// Should have the second value
		let result = layer.get(block, key).await.unwrap();
		assert_eq!(result.as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
	}

	// Tests for get_batch()
	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_empty_keys() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		let results = layer.get_batch(ctx.block_number(), &[]).await.unwrap();
		assert_eq!(results.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_local_modifications() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set_batch(&[(key1, Some(value1)), (key2, Some(value2))]).unwrap();

		let results = layer.get_batch(block, &[key1, key2]).await.unwrap();
		assert_eq!(results.len(), 2);
		assert_eq!(results[0].as_ref().and_then(|v| v.value.as_deref()), Some(value1.as_slice()));
		assert_eq!(results[1].as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_returns_none_for_deleted_prefix() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

		let results = layer.get_batch(block, &[key1.as_slice(), key2.as_slice()]).await.unwrap();
		assert!(results[0].is_some());
		assert!(results[1].is_some());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_local_overrides_parent() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
		let local_value = b"local_override";

		// Set one key locally
		layer.set(&key1, Some(local_value)).unwrap();

		let results = layer.get_batch(block, &[key1.as_slice(), key2.as_slice()]).await.unwrap();
		assert_eq!(
			results[0].as_ref().and_then(|v| v.value.as_deref()),
			Some(local_value.as_slice())
		);
		assert!(results[1].is_some());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_mixed_sources() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

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
			results[0].as_ref().and_then(|v| v.value.as_deref()),
			Some(b"local_value".as_slice())
		);
		assert_eq!(
			u32::decode(&mut &results[1].as_ref().unwrap().value.as_ref().unwrap()[..]).unwrap(),
			ctx.block_number()
		); // from parent
		assert!(results[2].as_ref().map(|v| v.value.is_none()).unwrap_or(false)); // deleted (has SharedValue with value: None)
		assert!(results[3].is_none()); // nonexistent
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_maintains_order() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

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
		assert_eq!(results[0].as_ref().and_then(|v| v.value.as_deref()), Some(value3.as_slice()));
		assert_eq!(results[1].as_ref().and_then(|v| v.value.as_deref()), Some(value1.as_slice()));
		assert_eq!(results[2].as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_retrieves_modified_value_from_fork_history() {
		let ctx = TestContext::for_local().await;
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
		let block_1 = layer.get_current_block_number() - 1;

		// Set and commit at block N+1
		layer
			.set_batch(&[(key1, Some(value1_block_2)), (key2, Some(value2_block_2))])
			.unwrap();
		layer.commit().await.unwrap();
		let block_2 = layer.get_current_block_number() - 1;

		// Query at block_1 - should get values from local_storage table
		let results_block_1 = layer.get_batch(block_1, &[key1, key2]).await.unwrap();
		assert_eq!(
			results_block_1[0],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: 0, // <- Comes from cache, not hot key, so set to 0
				value: Some(value1_block_1.to_vec())
			}))
		);
		assert_eq!(
			results_block_1[1],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: 0,
				value: Some(value2_block_1.to_vec())
			}))
		);

		// Query at block_2 - should get values from local_storage table
		let results_block_2 = layer.get_batch(block_2, &[key1, key2]).await.unwrap();
		assert_eq!(
			results_block_2[0],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: 0, // <- Comes from cache, not hot key, so set to 0
				value: Some(value1_block_2.to_vec())
			}))
		);
		assert_eq!(
			results_block_2[1],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: 0,
				value: Some(value2_block_2.to_vec())
			}))
		);

		// Query at latest block - should get values from modifications
		let results_latest =
			layer.get_batch(layer.get_current_block_number(), &[key1, key2]).await.unwrap();
		assert_eq!(
			results_latest[0],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: Some(value1_block_2.to_vec())
			}))
		);
		assert_eq!(
			results_latest[1],
			Some(Arc::new(LocalSharedValue {
				last_modification_block: block_2,
				value: Some(value2_block_2.to_vec())
			}))
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_retrieves_unmodified_value_from_remote_at_past_forked_block() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		let unmodified_key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let unmodified_key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

		// Advance a few blocks
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();
		let committed_block = layer.get_current_block_number() - 1;

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
			.get_batch(
				ctx.block_number(),
				&[unmodified_key1.as_slice(), unmodified_key2.as_slice()],
			)
			.await
			.unwrap();
		assert_eq!(results[0], remote_values[0]);
		assert_eq!(results[1], remote_values[1]);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_historical_block() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Wait for some blocks to be finalized
		std::thread::sleep(Duration::from_secs(30));

		// Query a block that's not in cache
		let block_number = ctx.block_number();
		let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
		let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
		let key3 = b"non_existent_key";

		// Get storage from historical block
		let results = layer
			.get_batch(block_number, &[key1.as_slice(), key2.as_slice(), key3])
			.await
			.unwrap();
		assert_eq!(results.len(), 3);
		assert_eq!(
			u32::decode(&mut &results[0].as_ref().unwrap().value.as_ref().unwrap()[..]).unwrap(),
			block_number
		);
		assert!(results[2].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_batch_non_existent_block_returns_none() {
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		// Test multiple scenarios:
		// 1. Latest block (from modifications)
		// 2. Historical block (from cache/RPC)

		// Advance some blocks
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();

		let latest_block_1 = layer.get_current_block_number();

		let key1 = b"local_key";
		let key2 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

		// Set a local modification
		layer.set(key1, Some(b"local_value")).unwrap();

		// Get from latest block (should hit modifications)
		let results1 = layer.get(latest_block_1, key1).await.unwrap();
		assert_eq!(
			results1.as_ref().and_then(|v| v.value.as_deref()),
			Some(b"local_value".as_slice())
		);

		// Get from historical block (should fetch and cache block)
		let historical_block = ctx.block_number();
		let results2 = layer
			.get(historical_block, key2.as_slice())
			.await
			.unwrap()
			.unwrap()
			.value
			.clone()
			.unwrap();
		assert_eq!(u32::decode(&mut &results2[..]).unwrap(), historical_block);

		// Commit block modifications
		layer.commit().await.unwrap();

		let latest_block_2 = layer.get_current_block_number();

		layer.set(key1, Some(b"local_value_2")).unwrap();

		let result_previous_block = layer.get(latest_block_1, key1).await.unwrap().unwrap();
		let result_latest_block = layer.get(latest_block_2, key1).await.unwrap().unwrap();

		assert_eq!(
			*result_previous_block,
			LocalSharedValue {
				last_modification_block: 0, /* <- This has been committed, so this comes from
				                             * cache and hence we're not interested in this
				                             * value, so it's set to 0 */
				value: Some(b"local_value".to_vec())
			}
		);
		assert_eq!(
			*result_latest_block,
			LocalSharedValue {
				last_modification_block: latest_block_2,
				value: Some(b"local_value_2".to_vec())
			}
		);
	}

	// Tests for set_batch()
	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_empty_entries() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		layer.set_batch(&[]).unwrap();

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_stores_multiple_values() {
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";

		layer.set_batch(&[(key1, Some(value1)), (key2, None)]).unwrap();

		let results = layer.get_batch(block, &[key1, key2]).await.unwrap();
		assert!(results[0].is_some());
		// Deleted keys return Some(SharedValue { value: None }) to distinguish from "not found"
		assert!(results[1].as_ref().map(|v| v.value.is_none()).unwrap_or(false));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_overwrites_previous_values() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key, Some(value1)).unwrap();
		layer.set_batch(&[(key, Some(value2))]).unwrap();

		let result = layer.get(block, key).await.unwrap();
		assert_eq!(result.as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_batch_duplicate_keys_last_wins() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key = b"key";
		let value1 = b"value1";
		let value2 = b"value2";

		// Set same key twice in one batch - last should win
		layer.set_batch(&[(key, Some(value1)), (key, Some(value2))]).unwrap();

		let result = layer.get(block, key).await.unwrap();
		assert_eq!(result.as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
	}

	// Tests for delete_prefix()
	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_removes_matching_keys() {
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";

		layer.delete_prefix(prefix).unwrap();

		// Should be marked as deleted
		assert!(layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn delete_prefix_with_empty_prefix() {
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";

		assert!(!layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn is_deleted_returns_true_after_delete() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		let prefix = b"prefix_";

		layer.delete_prefix(prefix).unwrap();

		assert!(layer.is_deleted(prefix).unwrap());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn is_deleted_exact_match_only() {
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn diff_returns_all_modifications() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		let key1 = b"key1";
		let key2 = b"key2";
		let value1 = b"value1";
		let value2 = b"value2";

		layer.set(key1, Some(value1)).unwrap();
		layer.set(key2, Some(value2)).unwrap();

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 2);
		assert!(diff.iter().any(|(k, v)| k == key1 &&
			v.as_ref().and_then(|v| v.value.as_deref()) == Some(value1.as_slice())));
		assert!(diff.iter().any(|(k, v)| k == key2 &&
			v.as_ref().and_then(|v| v.value.as_deref()) == Some(value2.as_slice())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn diff_includes_deletions() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		let key = b"deleted";

		layer.set(key, None).unwrap();

		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 1);
		assert_eq!(diff[0].0, key);
		// Deletion creates a SharedValue with value: None
		assert!(diff[0].1.as_ref().map(|v| v.value.is_none()).unwrap_or(false));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn diff_excludes_prefix_deleted_keys() {
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		let block = layer.get_current_block_number();

		let key1 = b"commit_key1";
		let key2 = b"commit_key2";
		let value1 = b"commit_value1";
		let value2 = b"commit_value2";

		// Set local modifications
		layer.set(key1, Some(value1)).unwrap();
		layer.set(key2, Some(value2)).unwrap();

		// Verify not in cache yet
		assert!(
			ctx.remote()
				.cache()
				.get_local_value_at_block(key1, block)
				.await
				.unwrap()
				.is_none()
		);
		assert!(
			ctx.remote()
				.cache()
				.get_local_value_at_block(key2, block)
				.await
				.unwrap()
				.is_none()
		);

		// Commit
		layer.commit().await.unwrap();

		// Verify now in cache at the block_number it was committed to
		let cached1 = ctx.remote().cache().get_local_value_at_block(key1, block).await.unwrap();
		let cached2 = ctx.remote().cache().get_local_value_at_block(key2, block).await.unwrap();

		assert_eq!(cached1, Some(Some(value1.to_vec())));
		assert_eq!(cached2, Some(Some(value2.to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_preserves_modifications() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		let block = layer.get_current_block_number();

		let key = b"preserve_key";
		let value = b"preserve_value";

		// Set and commit
		layer.set(key, Some(value)).unwrap();
		layer.commit().await.unwrap();

		// Modifications should still be in local layer
		let local_result = layer.get(block + 1, key).await.unwrap();
		assert_eq!(local_result.as_ref().and_then(|v| v.value.as_deref()), Some(value.as_slice()));

		// Should also be in diff
		let diff = layer.diff().unwrap();
		assert_eq!(diff.len(), 1);
		assert_eq!(diff[0].0, key);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_with_deletions() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key1 = b"delete_key1";
		let key2 = b"delete_key2";
		let value = b"value";

		// Set one value and mark another as deleted
		layer.set(key1, Some(value)).unwrap();
		layer.set(key2, None).unwrap();

		// Commit
		layer.commit().await.unwrap();

		// Both should be in cache
		let cached1 = ctx.remote().cache().get_local_value_at_block(key1, block).await.unwrap();
		let cached2 = ctx.remote().cache().get_local_value_at_block(key2, block).await.unwrap();

		assert_eq!(cached1, Some(Some(value.to_vec())));
		assert_eq!(cached2, Some(None)); // Cached as deletion (row exists with NULL value)
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_empty_modifications() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		// Commit with no modifications should succeed
		let result = layer.commit().await;
		assert!(result.is_ok());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_multiple_times() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);
		let block = layer.get_current_block_number();

		let key = b"multi_block_key";
		let value = b"multi_block_value";

		// Set local modification
		layer.set(key, Some(value)).unwrap();

		// Commit multiple times - each commit increments the block number
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();

		// Both block numbers should find the value in cache
		let cached1 = ctx.remote().cache().get_local_value_at_block(key, block).await.unwrap();
		let cached2 = ctx.remote().cache().get_local_value_at_block(key, block + 1).await.unwrap();

		assert_eq!(cached1, Some(Some(value.to_vec())));
		assert_eq!(cached2, Some(Some(value.to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_validity_ranges_work_properly() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		let key = b"validity_test_key";
		let value1 = b"value_version_1";
		let value2 = b"value_version_2";

		// Block N: Set initial value and commit
		let block_n = layer.get_current_block_number();
		layer.set(key, Some(value1)).unwrap();
		layer.commit().await.unwrap();

		// Verify key was created in local_keys
		let key_row = ctx.remote().cache().get_local_key(key).await.unwrap();
		assert!(key_row.is_some());
		let key_id = key_row.unwrap().id;

		// Verify value1 is valid from block_n onwards
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n).await.unwrap(),
			Some(Some(value1.to_vec()))
		);

		// Block N+1, N+2: Commit without changes (value should remain valid)
		layer.commit().await.unwrap();
		layer.commit().await.unwrap();

		// Value1 should still be valid at blocks N+1 and N+2
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n + 1).await.unwrap(),
			Some(Some(value1.to_vec()))
		);
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n + 2).await.unwrap(),
			Some(Some(value1.to_vec()))
		);

		// Block N+3: Update the value and commit
		layer.set(key, None).unwrap();
		layer.commit().await.unwrap();

		// Verify validity ranges:
		// - value1 should be valid from block_n to block_n_plus_3 (exclusive)
		// - value2 should be valid from block_n_plus_3 onwards
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n).await.unwrap(),
			Some(Some(value1.to_vec())),
		);
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n + 2).await.unwrap(),
			Some(Some(value1.to_vec())),
		);
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n + 3).await.unwrap(),
			Some(None),
		);
		assert_eq!(
			ctx.remote()
				.cache()
				.get_local_value_at_block(key, block_n + 3 + 10)
				.await
				.unwrap(),
			Some(None),
		);

		// Block N+4: Another update
		layer.set(key, Some(value2)).unwrap();
		layer.commit().await.unwrap();

		// Verify all three validity ranges
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n).await.unwrap(),
			Some(Some(value1.to_vec()))
		);
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n + 3).await.unwrap(),
			Some(None)
		);
		assert_eq!(
			ctx.remote().cache().get_local_value_at_block(key, block_n + 4).await.unwrap(),
			Some(Some(value2.to_vec()))
		);

		// Key ID should remain the same throughout
		let key_row_after = ctx.remote().cache().get_local_key(key).await.unwrap();
		assert_eq!(key_row_after.unwrap().id, key_id);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn commit_only_commits_the_minimal_information_needed() {
		use crate::schema::local_values::{self, columns as lvc};
		use diesel::prelude::*;
		use diesel_async::RunQueryDsl;
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);
		let cache_clone = layer.parent.cache().clone();

		let key1 = b"key_block_n";
		let key2 = b"key_block_n_plus_1";
		let value1 = b"value1";
		let value2 = b"value2";
		let value3 = b"value3";

		// Block N: Set key1 and commit
		let block_n = layer.get_current_block_number() as i64;
		layer.set(key1, Some(value1)).unwrap();
		layer.commit().await.unwrap();

		// Block N+1: Set key2 (key1 was set in previous block, shouldn't be re-committed)
		layer.set(key2, Some(value2)).unwrap();
		layer.commit().await.unwrap();

		// Empty ddbb, so the first commited keys have the first indices
		let key_1_id = 1;
		let key_2_id = 2;

		// The in_memory connection can only handle one connection per time. So as layer.commit()
		// needs one connection, we need to get the connection to directly query the ddbb just once
		// everything's committed, and drop it right after the queries for the next commit. That's
		// why this is inside its own scope
		{
			let mut conn = cache_clone.get_conn().await.unwrap();
			// Both keys have only one entry in the ddbb
			let key_1_entries: Vec<(i64, Option<i64>, Option<Vec<u8>>)> = local_values::table
				.filter(lvc::key_id.eq(key_1_id))
				.select((lvc::valid_from, lvc::valid_until, lvc::value))
				.load(&mut conn)
				.await
				.unwrap();

			let key_2_entries: Vec<(i64, Option<i64>, Option<Vec<u8>>)> = local_values::table
				.filter(lvc::key_id.eq(key_2_id))
				.select((lvc::valid_from, lvc::valid_until, lvc::value))
				.load(&mut conn)
				.await
				.unwrap();

			assert_eq!(key_1_entries.len(), 1);
			assert_eq!(key_1_entries[0], (block_n, None, Some(value1.to_vec())));
			assert_eq!(key_2_entries.len(), 1);
			assert_eq!(key_2_entries[0], (block_n + 1, None, Some(value2.to_vec())));
		}

		layer.set(key1, Some(value3)).unwrap();
		layer.commit().await.unwrap();

		{
			let mut conn = cache_clone.get_conn().await.unwrap();
			let key_1_entries: Vec<(i64, Option<i64>, Option<Vec<u8>>)> = local_values::table
				.filter(lvc::key_id.eq(key_1_id))
				.select((lvc::valid_from, lvc::valid_until, lvc::value))
				.load(&mut conn)
				.await
				.unwrap();

			let key_2_entries: Vec<(i64, Option<i64>, Option<Vec<u8>>)> = local_values::table
				.filter(lvc::key_id.eq(key_2_id))
				.select((lvc::valid_from, lvc::valid_until, lvc::value))
				.load(&mut conn)
				.await
				.unwrap();

			assert_eq!(key_1_entries.len(), 2);
			assert_eq!(key_1_entries[0], (block_n, Some(block_n + 2), Some(value1.to_vec())));
			assert_eq!(key_1_entries[1], (block_n + 2, None, Some(value3.to_vec())));
			assert_eq!(key_2_entries.len(), 1);
			assert_eq!(key_2_entries[0], (block_n + 1, None, Some(value2.to_vec())));
		}
	}

	// Tests for next_key()
	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_returns_next_key_from_parent() {
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Use a prefix that doesn't exist
		let nonexistent_prefix = b"nonexistent_prefix_12345";

		let result = layer.next_key(nonexistent_prefix, &[]).await.unwrap();
		assert!(result.is_none(), "Should return None for nonexistent prefix");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_skips_deleted_prefix() {
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
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
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Empty prefix should match all keys
		let result = layer.next_key(&[], &[]).await.unwrap();
		assert!(result.is_some(), "Empty prefix should return some key from storage");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_with_nonexistent_prefix() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		let nonexistent_prefix = b"this_prefix_definitely_does_not_exist_xyz";

		let result = layer.next_key(nonexistent_prefix, &[]).await.unwrap();
		assert!(result.is_none(), "Nonexistent prefix should return None");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn metadata_at_returns_metadata_for_future_blocks() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Metadata registered at fork point should be valid for future blocks too
		let future_block = ctx.block_number() + 100;
		let metadata = layer.metadata_at(future_block).await.unwrap();
		assert!(metadata.pallets().count() > 0, "Metadata should be valid for future blocks");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn metadata_at_fetches_from_remote_for_pre_fork_blocks() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// For blocks before fork point, metadata should be fetched from remote
		if ctx.block_number() > 0 {
			let metadata = layer.metadata_at(ctx.block_number() - 1).await.unwrap();
			assert!(metadata.pallets().count() > 0, "Should fetch metadata from remote");
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn register_metadata_version_adds_new_version() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Register a new metadata version at a future block
		let new_block = ctx.block_number() + 10;
		layer.register_metadata_version(new_block, ctx.metadata().clone()).unwrap();

		// Both versions should be accessible
		let old_metadata = layer.metadata_at(ctx.block_number()).await.unwrap();
		let new_metadata = layer.metadata_at(new_block).await.unwrap();

		assert!(old_metadata.pallets().count() > 0);
		assert!(new_metadata.pallets().count() > 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn register_metadata_version_respects_block_boundaries() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Register new metadata at block X+5
		let upgrade_block = ctx.block_number() + 5;
		layer.register_metadata_version(upgrade_block, ctx.metadata().clone()).unwrap();

		// Blocks before upgrade should get original metadata
		// Blocks at or after upgrade should get new metadata
		let before_upgrade = layer.metadata_at(upgrade_block - 1).await.unwrap();
		let at_upgrade = layer.metadata_at(upgrade_block).await.unwrap();
		let after_upgrade = layer.metadata_at(upgrade_block + 10).await.unwrap();

		// All should have pallets (same metadata in this test, but different Arc instances
		// after upgrade point)
		assert!(before_upgrade.pallets().count() > 0);
		assert!(at_upgrade.pallets().count() > 0);
		assert!(after_upgrade.pallets().count() > 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn has_code_changed_at_returns_false_when_no_code_modified() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// No modifications made, should return false
		let result = layer.has_code_changed_at(ctx.block_number()).unwrap();
		assert!(!result, "Should return false when no code was modified");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn has_code_changed_at_returns_false_for_non_code_modifications() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Modify a non-code key
		layer.set(b"some_random_key", Some(b"some_value")).unwrap();

		let block = layer.get_current_block_number();
		let result = layer.has_code_changed_at(block).unwrap();
		assert!(!result, "Should return false when only non-code keys modified");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn has_code_changed_at_returns_true_when_code_modified() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Modify the :code key
		let code_key = sp_core::storage::well_known_keys::CODE;
		layer.set(code_key, Some(b"new_runtime_code")).unwrap();

		let block = layer.get_current_block_number();
		let result = layer.has_code_changed_at(block).unwrap();
		assert!(result, "Should return true when :code was modified at the specified block");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn has_code_changed_at_returns_false_for_different_block() {
		let ctx = TestContext::for_local().await;
		let layer = create_layer(&ctx);

		// Modify the :code key at current block
		let code_key = sp_core::storage::well_known_keys::CODE;
		layer.set(code_key, Some(b"new_runtime_code")).unwrap();

		let current_block = layer.get_current_block_number();

		// Check a different block number - should return false
		let result = layer.has_code_changed_at(current_block + 1).unwrap();
		assert!(!result, "Should return false when checking different block than modification");

		let result = layer.has_code_changed_at(current_block - 1).unwrap();
		assert!(!result, "Should return false when checking block before modification");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn has_code_changed_at_tracks_modification_block_correctly() {
		let ctx = TestContext::for_local().await;
		let mut layer = create_layer(&ctx);

		let code_key = sp_core::storage::well_known_keys::CODE;
		let first_block = layer.get_current_block_number();

		// Modify code at first block
		layer.set(code_key, Some(b"runtime_v1")).unwrap();
		assert!(
			layer.has_code_changed_at(first_block).unwrap(),
			"Code should be marked as changed at first block"
		);

		// Commit and advance to next block
		layer.commit().await.unwrap();
		let second_block = layer.get_current_block_number();

		// Code was modified at first_block, not second_block
		assert!(
			layer.has_code_changed_at(first_block).unwrap(),
			"Code change should still be recorded at first block"
		);
		assert!(
			!layer.has_code_changed_at(second_block).unwrap(),
			"Code should not be marked as changed at second block (no new modification)"
		);

		// Modify code again at second block
		layer.set(code_key, Some(b"runtime_v2")).unwrap();
		assert!(
			layer.has_code_changed_at(second_block).unwrap(),
			"Code should be marked as changed at second block after new modification"
		);
	}
}
