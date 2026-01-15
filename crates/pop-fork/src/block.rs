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
use subxt::{config::substrate::H256, ext::codec::Encode};
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
	storage: LocalStorageLayer,
	/// The parent block. Keeping blocks in memory is cheap as the `LocalStorageLayer` is shared
	/// between all fork-produced blocks.
	pub parent: Option<Box<Block>>,
}

/// Handy type to allow specifying both number and hash as the fork point.
pub enum BlockForkPoint {
	Number(u32),
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

		// Create storage layers
		let remote = RemoteStorageLayer::new(rpc, cache);
		let storage = LocalStorageLayer::new(remote, block_number, block_hash);

		// Encode header for storage
		let header_encoded = header.encode();

		Ok(Self {
			number: block_number,
			hash: block_hash,
			parent_hash,
			header: header_encoded,
			extrinsics: vec![], // Fork point has no new extrinsics
			storage,
			parent: None,
		})
	}

	/// Create a new child block with the given metadata and storage.
	///
	/// This is a lower-level constructor for creating blocks with explicit
	/// parameters.
	///
	/// # Arguments
	///
	/// * `hash` - The block hash
	/// * `header` - The encoded block header
	/// * `extrinsics` - The extrinsics (transactions) in this block
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

	/// Tests that spawn local test nodes.
	///
	/// These tests are run sequentially via nextest configuration to avoid
	/// concurrent node downloads causing race conditions.
	mod sequential {
		use super::*;
		use crate::StorageCache;
		use pop_common::test_env::TestNode;

		/// Helper struct to hold the test node and context together.
		struct TestContext {
			#[allow(dead_code)]
			node: TestNode,
			endpoint: Url,
			cache: StorageCache,
			block_hash: H256,
			block_number: u32,
			rpc: ForkRpcClient,
		}

		async fn create_test_context() -> TestContext {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let rpc = ForkRpcClient::connect(&endpoint).await.unwrap();
			let block_hash = rpc.finalized_head().await.unwrap();
			let header = rpc.header(block_hash).await.unwrap();
			let block_number = header.number;
			let cache = StorageCache::in_memory().await.unwrap();
			TestContext { node, endpoint, cache, block_hash, block_number, rpc }
		}

		#[tokio::test]
		async fn fork_point_with_hash_creates_block_with_correct_metadata() {
			let ctx = create_test_context().await;

			let expected_parent_hash = ctx.rpc.header(ctx.block_hash).await.unwrap().parent_hash;

			let block = Block::fork_point(&ctx.endpoint, ctx.cache, ctx.block_hash.into())
				.await
				.unwrap();

			assert_eq!(block.number, ctx.block_number);
			assert_eq!(block.hash, ctx.block_hash);
			assert_eq!(block.parent_hash, expected_parent_hash);
			assert!(!block.header.is_empty());
			assert!(block.extrinsics.is_empty());
			assert!(block.parent.is_none());
		}

		#[tokio::test]
		async fn fork_point_with_non_existent_hash_returns_error() {
			let ctx = create_test_context().await;
			let non_existent_hash = H256::from([0xde; 32]);

			let result =
				Block::fork_point(&ctx.endpoint, ctx.cache, non_existent_hash.into()).await;

			assert!(
				matches!(result, Err(BlockError::BlockHashNotFound(h)) if h == non_existent_hash)
			);
		}

		#[tokio::test]
		async fn fork_point_with_number_creates_block_with_correct_metadata() {
			let ctx = create_test_context().await;
			let expected_parent_hash = ctx.rpc.header(ctx.block_hash).await.unwrap().parent_hash;

			let block = Block::fork_point(&ctx.endpoint, ctx.cache, ctx.block_number.into())
				.await
				.unwrap();

			assert_eq!(block.number, ctx.block_number);
			assert_eq!(block.hash, ctx.block_hash);
			assert_eq!(block.parent_hash, expected_parent_hash);
			assert!(!block.header.is_empty());
			assert!(block.extrinsics.is_empty());
			assert!(block.parent.is_none());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fork_point_with_non_existent_number_returns_error() {
			let ctx = create_test_context().await;
			let non_existent_number = u32::MAX;

			let result =
				Block::fork_point(&ctx.endpoint, ctx.cache, non_existent_number.into()).await;

			assert!(
				matches!(result, Err(BlockError::BlockNumberNotFound(n)) if n == non_existent_number)
			);
		}

		#[tokio::test]
		async fn child_creates_block_with_correct_metadata() {
			let ctx = create_test_context().await;
			let mut parent = Block::fork_point(&ctx.endpoint, ctx.cache, ctx.block_hash.into())
				.await
				.unwrap();

			let child_hash = H256::from([0x42; 32]);
			let child_header = vec![1, 2, 3, 4];
			let child_extrinsics = vec![vec![5, 6, 7]];

			let child = parent
				.child(child_hash, child_header.clone(), child_extrinsics.clone())
				.await
				.unwrap();

			assert_eq!(child.number, parent.number + 1);
			assert_eq!(child.hash, child_hash);
			assert_eq!(child.parent_hash, parent.hash);
			assert_eq!(child.header, child_header);
			assert_eq!(child.extrinsics, child_extrinsics);
			assert_eq!(child.parent.unwrap().number, parent.number);
		}

		#[tokio::test]
		async fn child_commits_parent_storage() {
			let ctx = create_test_context().await;
			let mut parent = Block::fork_point(&ctx.endpoint, ctx.cache, ctx.block_hash.into())
				.await
				.unwrap();

			let key = b"committed_key";
			let value = b"committed_value";

			// Set value on parent
			parent.storage_mut().set(key, Some(value)).unwrap();

			// Create child (this commits parent storage)
			let mut child = parent.child(H256::from([0x42; 32]), vec![], vec![]).await.unwrap();

			let value2 = b"committed_value2";

			child.storage_mut().set(key, Some(value2)).unwrap();

			// Value should be readable both at child and parent block_numbers
			assert_eq!(
				child.storage().get(child.number, key).await.unwrap().as_deref().unwrap(),
				value2
			);
			assert_eq!(
				child.storage().get(parent.number, key).await.unwrap().as_deref().unwrap(),
				value
			);
		}

		#[tokio::test]
		async fn child_storage_inherits_parent_modifications() {
			let ctx = create_test_context().await;
			let mut parent = Block::fork_point(&ctx.endpoint, ctx.cache, ctx.block_hash.into())
				.await
				.unwrap();

			let key = b"inherited_key";
			let value = b"inherited_value";

			parent.storage_mut().set(key, Some(value)).unwrap();

			let child = parent.child(H256::from([0x42; 32]), vec![], vec![]).await.unwrap();

			// Child should see the value at its block number
			assert_eq!(
				child.storage().get(child.number, key).await.unwrap().as_deref().unwrap(),
				value
			);
		}
	}
}
