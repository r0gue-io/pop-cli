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

		// Fetch and decode metadata from remote
		let metadata = self.parent.rpc().metadata(block_hash).await?;

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

	/// Set a storage value visible from the fork point onwards.
	///
	/// Unlike [`Self::set`], which records the modification at the current working block,
	/// this marks the entry with `last_modification_block = first_forked_block_number - 1`
	/// (saturating at 0) so it is visible immediately at the fork head and for any later
	/// fork-local query, but not for historical pre-fork queries.
	/// This is used for injecting initial state (e.g., dev accounts, sudo key)
	/// that should be readable before any block is built.
	///
	/// These entries are never committed to the persistent cache by [`Self::commit`]
	/// (which only commits entries at `current_block_number`), but they remain in
	/// the in-memory modifications map for the lifetime of the fork.
	pub fn set_initial(&self, key: &[u8], value: Option<&[u8]>) -> Result<(), LocalStorageError> {
		let mut modifications_lock =
			self.modifications.write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let initial_visibility_block = self.first_forked_block_number.saturating_sub(ONE_BLOCK);

		modifications_lock.insert(
			key.to_vec(),
			Some(Arc::new(LocalSharedValue {
				last_modification_block: initial_visibility_block,
				value: value.map(|v| v.to_vec()),
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

	/// Batch version of [`Self::set_initial`].
	///
	/// Sets multiple storage values visible from the fork point onwards, using
	/// `last_modification_block = first_forked_block_number - 1` (saturating at 0).
	/// See [`Self::set_initial`]
	/// for details.
	pub fn set_batch_initial(
		&self,
		entries: &[(&[u8], Option<&[u8]>)],
	) -> Result<(), LocalStorageError> {
		if entries.is_empty() {
			return Ok(());
		}

		let mut modifications_lock =
			self.modifications.write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let initial_visibility_block = self.first_forked_block_number.saturating_sub(ONE_BLOCK);

		for (key, value) in entries {
			modifications_lock.insert(
				key.to_vec(),
				Some(Arc::new(LocalSharedValue {
					last_modification_block: initial_visibility_block,
					value: value.map(|v| v.to_vec()),
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

		// Commit all changes in a single transaction
		self.parent
			.cache()
			.commit_local_changes(&entries_to_commit, current_block_number)
			.await?;

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
