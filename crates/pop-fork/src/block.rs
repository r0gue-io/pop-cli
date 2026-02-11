// SPDX-License-Identifier: GPL-3.0

//! Block structure for forked blockchain state.
//!
//! This module provides the [`Block`] struct which represents a single block
//! in a forked blockchain. Each block contains its metadata (number, hash,
//! parent hash, header, extrinsics) and an associated storage layer for
//! reading and modifying state.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                           Block                                  │
//! │                                                                   │
//! │   ┌──────────────────────────────────────────────────────────┐   │
//! │   │ Metadata: number, hash, parent_hash, header, extrinsics  │   │
//! │   └──────────────────────────────────────────────────────────┘   │
//! │                              │                                    │
//! │                              ▼                                    │
//! │   ┌──────────────────────────────────────────────────────────┐   │
//! │   │                  LocalStorageLayer                        │   │
//! │   │  (tracks modifications on top of remote chain state)      │   │
//! │   └──────────────────────────────────────────────────────────┘   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use pop_fork::{Block, ForkRpcClient, StorageCache};
//!
//! // Create a fork point from a live chain
//! let rpc = ForkRpcClient::connect(&endpoint).await?;
//! let cache = StorageCache::in_memory().await?;
//! let block_hash = rpc.finalized_head().await?;
//! let fork_block = Block::fork_point(rpc, cache, block_hash).await?;
//!
//! // Access storage
//! let value = fork_block.storage().get(fork_block.number, &key).await?;
//!
//! // Modify storage and commit to create a new block
//! fork_block.storage().set(&key, Some(&new_value))?;
//! ```

use crate::{BlockError, ForkRpcClient, LocalStorageLayer, RemoteStorageLayer, StorageCache};
use std::sync::Arc;
use subxt::{Metadata, config::substrate::H256, ext::codec::Encode};
use url::Url;

/// A block in a forked blockchain.
///
/// Represents a single block with its metadata and associated storage state.
/// Blocks can be created as fork points from live chains or as child blocks
/// extending an existing fork.
///
/// # Storage Model
///
/// Each block has an associated [`LocalStorageLayer`] that tracks storage
/// modifications. The storage layer uses a layered architecture:
///
/// - **Local modifications**: In-memory changes for the current block
/// - **Committed state**: Previously committed blocks stored in SQLite
/// - **Remote state**: Original chain state fetched lazily via RPC + cache for faster relaunches.
///
/// # Cloning
///
/// `Block` is cheap to clone, as `LocalStorageLayer` is cheap to clone.
#[derive(Clone, Debug)]
pub struct Block {
	/// The block number (height).
	pub number: u32,
	/// The block hash.
	pub hash: H256,
	/// The parent block hash.
	pub parent_hash: H256,
	/// The encoded block header.
	pub header: Vec<u8>,
	/// The extrinsics (transactions) in this block.
	pub extrinsics: Vec<Vec<u8>>,
	/// The storage layer for this block.
	///
	/// Also manages runtime metadata versions, enabling dynamic lookup of
	/// pallet and call indices for inherent providers.
	storage: LocalStorageLayer,
	/// The parent block. Keeping blocks in memory is cheap as the `LocalStorageLayer` is shared
	/// between all fork-produced blocks.
	pub parent: Option<Box<Block>>,
}

/// Handy type to allow specifying both number and hash as the fork point.
#[derive(Clone, Copy)]
pub enum BlockForkPoint {
	/// Fork at a specific block number.
	Number(u32),
	/// Fork at a specific block hash.
	Hash(H256),
}

impl From<u32> for BlockForkPoint {
	fn from(number: u32) -> Self {
		Self::Number(number)
	}
}

impl From<H256> for BlockForkPoint {
	fn from(hash: H256) -> Self {
		Self::Hash(hash)
	}
}

impl Block {
	/// Create a new block at a fork point from a live chain.
	///
	/// This is the entry point for creating a forked chain. It fetches the block
	/// header from the remote chain and sets up a [`LocalStorageLayer`] for tracking
	/// subsequent modifications.
	///
	/// # Arguments
	///
	/// * `endpoint` - RPC client url.
	/// * `cache` - Storage cache for persisting fetched and modified values
	/// * `block_fork_point` - Hash or number of the block to fork from
	///
	/// # Returns
	///
	/// A new `Block` representing the fork point, with an empty extrinsics list
	/// (since we're forking from existing chain state, not producing new blocks).
	pub async fn fork_point(
		endpoint: &Url,
		cache: StorageCache,
		block_fork_point: BlockForkPoint,
	) -> Result<Self, BlockError> {
		// Fetch header from remote chain
		let rpc = ForkRpcClient::connect(endpoint).await?;
		let (block_hash, header) = match block_fork_point {
			BlockForkPoint::Number(block_number) => {
				let (block_hash, block) =
					if let Some(block_by_number) = rpc.block_by_number(block_number).await? {
						block_by_number
					} else {
						return Err(BlockError::BlockNumberNotFound(block_number));
					};
				(block_hash, block.header)
			},
			BlockForkPoint::Hash(block_hash) => (
				block_hash,
				rpc.header(block_hash)
					.await
					.map_err(|_| BlockError::BlockHashNotFound(block_hash))?,
			),
		};
		let block_number = header.number;
		let parent_hash = header.parent_hash;

		// Fetch full block to get extrinsics (needed for parachain inherents)
		let extrinsics = rpc
			.block_by_hash(block_hash)
			.await?
			.map(|block| block.extrinsics.into_iter().map(|ext| ext.0.to_vec()).collect::<Vec<_>>())
			.unwrap_or_default();

		// Fetch and decode runtime metadata
		let metadata = rpc.metadata(block_hash).await?;

		// Create storage layers (metadata is stored in LocalStorageLayer)
		let remote = RemoteStorageLayer::new(rpc, cache);
		let storage = LocalStorageLayer::new(remote, block_number, block_hash, metadata);

		// Encode header for storage
		let header_encoded = header.encode();

		Ok(Self {
			number: block_number,
			hash: block_hash,
			parent_hash,
			header: header_encoded,
			extrinsics, // Extrinsics from the forked block (needed for parachain inherents)
			storage,
			parent: None,
		})
	}

	/// Create a new child block with the given hash, header, and extrinsics.
	///
	/// This commits the parent's storage modifications and creates a new block
	/// that shares the same storage layer (including metadata versions).
	///
	/// # Arguments
	///
	/// * `hash` - The block hash
	/// * `header` - The encoded block header
	/// * `extrinsics` - The extrinsics (transactions) in this block
	///
	/// # Note
	///
	/// The child block shares the same storage layer as the parent, including
	/// metadata versions. If a runtime upgrade occurred (`:code` storage changed),
	/// the new metadata should be registered via `storage.register_metadata_version()`.
	pub async fn child(
		&mut self,
		hash: H256,
		header: Vec<u8>,
		extrinsics: Vec<Vec<u8>>,
	) -> Result<Self, BlockError> {
		self.storage.commit().await?;
		Ok(Self {
			number: self.number + 1,
			hash,
			parent_hash: self.hash,
			header,
			extrinsics,
			storage: self.storage.clone(),
			parent: Some(Box::new(self.clone())),
		})
	}

	/// Create a mocked Block for executing runtime calls on historical blocks.
	///
	/// This block uses the real block hash and number (for correct storage queries)
	/// but placeholder values for other fields since the executor only needs storage access.
	/// The storage layer delegates to remote for historical data.
	///
	/// # Arguments
	///
	/// * `hash` - The real block hash being queried
	/// * `number` - The actual block number (needed for correct storage queries)
	/// * `storage` - Storage layer that delegates to remote for historical data
	pub fn mocked_for_call(hash: H256, number: u32, storage: LocalStorageLayer) -> Self {
		Self {
			number,
			hash,
			parent_hash: H256::zero(),
			header: vec![],
			extrinsics: vec![],
			storage,
			parent: None,
		}
	}

	/// Get a reference to the storage layer.
	///
	/// Use this to read storage values at this block's height.
	///
	/// # Example
	///
	/// ```ignore
	/// let value = block.storage().get(block.number, &key).await?;
	/// ```
	pub fn storage(&self) -> &LocalStorageLayer {
		&self.storage
	}

	/// Get a mutable reference to the storage layer.
	///
	/// Use this to modify storage values. Modifications are tracked locally
	/// and can be committed using [`LocalStorageLayer::commit`].
	///
	/// # Example
	///
	/// ```ignore
	/// block.storage_mut().set(&key, Some(&value))?;
	/// block.storage_mut().commit().await?;
	/// ```
	pub fn storage_mut(&mut self) -> &mut LocalStorageLayer {
		&mut self.storage
	}

	/// Get the runtime metadata for this block.
	///
	/// This provides access to pallet and call indices for dynamic extrinsic
	/// encoding. Use this in inherent providers to look up pallet indices
	/// instead of relying on hardcoded values.
	///
	/// Returns an `Arc<Metadata>` which can be used like a reference (via `Deref`).
	/// The metadata is shared across all blocks that use the same runtime version,
	/// avoiding unnecessary cloning.
	///
	/// # Example
	///
	/// ```ignore
	/// let metadata = block.metadata().await?;
	/// let pallet = metadata.pallet_by_name("Timestamp")?;
	/// let pallet_index = pallet.index();
	/// let call_variant = pallet.call_variant_by_name("set")?;
	/// let call_index = call_variant.index;
	/// ```
	pub async fn metadata(&self) -> Result<Arc<Metadata>, BlockError> {
		Ok(self.storage.metadata_at(self.number).await?)
	}

	/// Get the runtime code (`:code`) for this block.
	///
	/// Retrieves the WASM runtime code from the storage layer, which handles
	/// the layered lookup (local modifications → cache → remote).
	///
	/// # Returns
	///
	/// The runtime WASM code as bytes.
	///
	/// # Errors
	///
	/// Returns [`BlockError::RuntimeCodeNotFound`] if the `:code` key is not
	/// found in storage.
	///
	/// # Example
	///
	/// ```ignore
	/// let runtime_code = block.runtime_code().await?;
	/// let executor = RuntimeExecutor::new(runtime_code)?;
	/// ```
	pub async fn runtime_code(&self) -> Result<Vec<u8>, BlockError> {
		let code_key = sp_core::storage::well_known_keys::CODE;
		self.storage()
			.get(self.number, code_key)
			.await?
			.and_then(|v| v.value.clone())
			.ok_or(BlockError::RuntimeCodeNotFound)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn from_u32_creates_number_variant() {
		let fork_point: BlockForkPoint = 42u32.into();
		assert!(matches!(fork_point, BlockForkPoint::Number(42)));
	}

	#[test]
	fn from_h256_creates_hash_variant() {
		let hash = H256::from([0xab; 32]);
		let fork_point: BlockForkPoint = hash.into();
		assert!(matches!(fork_point, BlockForkPoint::Hash(h) if h == hash));
	}
}
