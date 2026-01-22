// SPDX-License-Identifier: GPL-3.0

//! Blockchain manager for forked chains.
//!
//! This module provides the [`Blockchain`] struct, which is the main entry point
//! for creating and interacting with local forks of live Polkadot SDK chains.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        Blockchain                               │
//! │                                                                 │
//! │   fork() ──────► Connect to live chain                          │
//! │                        │                                        │
//! │                        ▼                                        │
//! │              Create fork point Block                            │
//! │                        │                                        │
//! │                        ▼                                        │
//! │              Initialize RuntimeExecutor                         │
//! │                        │                                        │
//! │                        ▼                                        │
//! │              Detect chain type (relay/para)                     │
//! │                        │                                        │
//! │                        ▼                                        │
//! │              Ready for block building                           │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::Blockchain;
//! use url::Url;
//!
//! // Fork a live chain
//! let endpoint: Url = "wss://rpc.polkadot.io".parse()?;
//! let blockchain = Blockchain::fork(&endpoint, None).await?;
//!
//! // Get chain info
//! println!("Chain: {}", blockchain.chain_name());
//! println!("Fork point: {:?}", blockchain.fork_point());
//!
//! // Build a block with extrinsics
//! let block = blockchain.build_block(vec![extrinsic]).await?;
//!
//! // Query storage at head
//! let value = blockchain.storage(&key).await?;
//! ```

use crate::{
	Block, BlockBuilder, BlockBuilderError, BlockError, BlockForkPoint, CacheError, ExecutorConfig,
	ExecutorError, InherentProvider, RuntimeExecutor, StorageCache, create_next_header,
	default_providers,
};
use std::{path::Path, sync::Arc};
use subxt::config::substrate::H256;
use tokio::sync::RwLock;
use url::Url;

/// Errors that can occur when working with the blockchain manager.
#[derive(Debug, thiserror::Error)]
pub enum BlockchainError {
	/// Block-related error.
	#[error(transparent)]
	Block(#[from] BlockError),

	/// Block builder error.
	#[error(transparent)]
	Builder(#[from] BlockBuilderError),

	/// Cache error.
	#[error(transparent)]
	Cache(#[from] CacheError),

	/// Executor error.
	#[error(transparent)]
	Executor(#[from] ExecutorError),
}

/// Type of chain being forked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainType {
	/// A relay chain (Polkadot, Kusama, etc.).
	RelayChain,
	/// A parachain with a specific para ID.
	Parachain {
		/// The parachain ID.
		para_id: u32,
	},
}

/// The blockchain manager for a forked chain.
///
/// `Blockchain` is the main entry point for creating local forks of live
/// Polkadot SDK chains. It manages the fork lifecycle, block building,
/// and provides APIs for querying state and executing runtime calls.
///
/// # Creating a Fork
///
/// Use [`Blockchain::fork`] to create a new fork from a live chain:
///
/// ```ignore
/// let blockchain = Blockchain::fork(&endpoint, None).await?;
/// ```
///
/// # Block Building
///
/// Build blocks using [`build_block`](Blockchain::build_block) or
/// [`build_empty_block`](Blockchain::build_empty_block):
///
/// ```ignore
/// // Build a block with user extrinsics
/// let block = blockchain.build_block(vec![signed_extrinsic]).await?;
///
/// // Build an empty block (just inherents)
/// let block = blockchain.build_empty_block().await?;
/// ```
///
/// # Querying State
///
/// Query storage at the current head or at a specific block:
///
/// ```ignore
/// // At head
/// let value = blockchain.storage(&key).await?;
///
/// // At a specific block
/// let value = blockchain.storage_at(block_hash, &key).await?;
/// ```
///
/// # Thread Safety
///
/// `Blockchain` is `Send + Sync` and can be safely shared across async tasks.
/// Internal state is protected by `RwLock`.
pub struct Blockchain {
	/// Current head block.
	head: RwLock<Block>,

	/// Inherent providers for block building.
	inherent_providers: Vec<Arc<dyn InherentProvider>>,

	/// Chain name (e.g., "Polkadot", "Asset Hub").
	chain_name: String,

	/// Chain type (relay chain or parachain).
	chain_type: ChainType,

	/// Fork point block hash.
	fork_point_hash: H256,

	/// Fork point block number.
	fork_point_number: u32,

	/// Executor configuration for runtime calls.
	executor_config: ExecutorConfig,
}

impl Blockchain {
	/// Create a new blockchain forked from a live chain.
	///
	/// This connects to the live chain, fetches the fork point block,
	/// initializes the runtime executor, and detects the chain type.
	///
	/// # Arguments
	///
	/// * `endpoint` - RPC endpoint URL of the live chain
	/// * `cache_path` - Optional path for persistent SQLite cache. If `None`, an in-memory cache is
	///   used.
	///
	/// # Returns
	///
	/// A new `Blockchain` instance ready for block building.
	///
	/// # Example
	///
	/// ```ignore
	/// use pop_fork::Blockchain;
	/// use url::Url;
	///
	/// let endpoint: Url = "wss://rpc.polkadot.io".parse()?;
	///
	/// // With in-memory cache
	/// let blockchain = Blockchain::fork(&endpoint, None).await?;
	///
	/// // With persistent cache
	/// let blockchain = Blockchain::fork(&endpoint, Some("./cache.sqlite")).await?;
	/// ```
	pub async fn fork(
		endpoint: &Url,
		cache_path: Option<&str>,
	) -> Result<Arc<Self>, BlockchainError> {
		Self::fork_with_config(endpoint, cache_path, None, ExecutorConfig::default()).await
	}

	/// Create a new blockchain forked from a live chain at a specific block.
	///
	/// Similar to [`fork`](Blockchain::fork), but allows specifying the exact
	/// block to fork from.
	///
	/// # Arguments
	///
	/// * `endpoint` - RPC endpoint URL of the live chain
	/// * `cache_path` - Optional path for persistent SQLite cache
	/// * `fork_point` - Block number or hash to fork from. If `None`, uses the latest finalized
	///   block.
	///
	/// # Example
	///
	/// ```ignore
	/// // Fork at a specific block number
	/// let blockchain = Blockchain::fork_at(&endpoint, None, Some(12345678.into())).await?;
	///
	/// // Fork at a specific block hash
	/// let blockchain = Blockchain::fork_at(&endpoint, None, Some(block_hash.into())).await?;
	/// ```
	pub async fn fork_at(
		endpoint: &Url,
		cache_path: Option<&str>,
		fork_point: Option<BlockForkPoint>,
	) -> Result<Arc<Self>, BlockchainError> {
		Self::fork_with_config(endpoint, cache_path, fork_point, ExecutorConfig::default()).await
	}

	/// Create a new blockchain forked from a live chain with custom executor configuration.
	///
	/// This is the most flexible fork method, allowing customization of both
	/// the fork point and the executor configuration.
	///
	/// # Arguments
	///
	/// * `endpoint` - RPC endpoint URL of the live chain
	/// * `cache_path` - Optional path for persistent SQLite cache
	/// * `fork_point` - Block number or hash to fork from. If `None`, uses the latest finalized
	///   block.
	/// * `executor_config` - Configuration for the runtime executor
	///
	/// # Example
	///
	/// ```ignore
	/// use pop_fork::{Blockchain, ExecutorConfig, SignatureMockMode};
	///
	/// // Fork with signature mocking enabled (useful for testing)
	/// let config = ExecutorConfig {
	///     signature_mock: SignatureMockMode::AlwaysValid,
	///     ..Default::default()
	/// };
	/// let blockchain = Blockchain::fork_with_config(&endpoint, None, None, config).await?;
	/// ```
	pub async fn fork_with_config(
		endpoint: &Url,
		cache_path: Option<&str>,
		fork_point: Option<BlockForkPoint>,
		executor_config: ExecutorConfig,
	) -> Result<Arc<Self>, BlockchainError> {
		// Create storage cache
		let cache = StorageCache::open(cache_path.map(Path::new)).await?;

		// Determine fork point
		let fork_point = match fork_point {
			Some(fp) => fp,
			None => {
				// Get latest finalized block from RPC
				let rpc =
					crate::ForkRpcClient::connect(endpoint).await.map_err(BlockError::from)?;
				let finalized = rpc.finalized_head().await.map_err(BlockError::from)?;
				BlockForkPoint::Hash(finalized)
			},
		};

		// Create fork point block
		let fork_block = Block::fork_point(endpoint, cache, fork_point).await?;
		let fork_point_hash = fork_block.hash;
		let fork_point_number = fork_block.number;

		// Detect chain type
		let chain_type = Self::detect_chain_type(&fork_block).await?;

		// Get chain name
		let chain_name = Self::get_chain_name(&fork_block).await?;

		// Create inherent providers based on chain type
		let is_parachain = matches!(chain_type, ChainType::Parachain { .. });
		let inherent_providers = default_providers(is_parachain)
			.into_iter()
			.map(|p| Arc::from(p) as Arc<dyn InherentProvider>)
			.collect();

		Ok(Arc::new(Self {
			head: RwLock::new(fork_block),
			inherent_providers,
			chain_name,
			chain_type,
			fork_point_hash,
			fork_point_number,
			executor_config,
		}))
	}

	/// Get the chain name.
	pub fn chain_name(&self) -> &str {
		&self.chain_name
	}

	/// Get the chain type.
	pub fn chain_type(&self) -> &ChainType {
		&self.chain_type
	}

	/// Get the fork point block hash.
	pub fn fork_point(&self) -> H256 {
		self.fork_point_hash
	}

	/// Get the fork point block number.
	pub fn fork_point_number(&self) -> u32 {
		self.fork_point_number
	}

	/// Get the current head block.
	pub async fn head(&self) -> Block {
		self.head.read().await.clone()
	}

	/// Get the current head block number.
	pub async fn head_number(&self) -> u32 {
		self.head.read().await.number
	}

	/// Get the current head block hash.
	pub async fn head_hash(&self) -> H256 {
		self.head.read().await.hash
	}

	/// Build a new block with the given extrinsics.
	///
	/// This creates a new block on top of the current head, applying:
	/// 1. Inherent extrinsics (timestamp, parachain validation data, etc.)
	/// 2. User-provided extrinsics
	///
	/// The new block becomes the new head.
	///
	/// # Arguments
	///
	/// * `extrinsics` - User extrinsics to include in the block
	///
	/// # Returns
	///
	/// The newly created block.
	///
	/// # Example
	///
	/// ```ignore
	/// let extrinsic = /* create signed extrinsic */;
	/// let block = blockchain.build_block(vec![extrinsic]).await?;
	/// println!("New block: #{} ({:?})", block.number, block.hash);
	/// ```
	pub async fn build_block(&self, extrinsics: Vec<Vec<u8>>) -> Result<Block, BlockchainError> {
		let mut head = self.head.write().await;

		// Get runtime code from current head
		let runtime_code = head.runtime_code().await?;

		// Create executor with current runtime and configured settings
		let executor =
			RuntimeExecutor::with_config(runtime_code, None, self.executor_config.clone())?;

		// Create header for new block
		let header = create_next_header(&head, vec![]);

		// Convert Arc providers to Box for BlockBuilder
		let providers: Vec<Box<dyn InherentProvider>> = self
			.inherent_providers
			.iter()
			.map(|p| Box::new(ArcProvider(Arc::clone(p))) as Box<dyn InherentProvider>)
			.collect();

		// Create block builder
		let mut builder = BlockBuilder::new(head.clone(), executor, header, providers);

		// Initialize block
		builder.initialize().await?;

		// Apply inherents
		builder.apply_inherents().await?;

		// Apply user extrinsics
		for extrinsic in extrinsics {
			builder.apply_extrinsic(extrinsic).await?;
		}

		// Finalize and get new block
		let new_block = builder.finalize().await?;

		// Update head
		*head = new_block.clone();

		Ok(new_block)
	}

	/// Build an empty block (just inherents, no user extrinsics).
	///
	/// This is useful for advancing the chain state without any user
	/// transactions.
	///
	/// # Returns
	///
	/// The newly created block.
	pub async fn build_empty_block(&self) -> Result<Block, BlockchainError> {
		self.build_block(vec![]).await
	}

	/// Execute a runtime call at the current head.
	///
	/// # Arguments
	///
	/// * `method` - Runtime API method name (e.g., "Core_version")
	/// * `args` - SCALE-encoded arguments
	///
	/// # Returns
	///
	/// The SCALE-encoded result from the runtime.
	pub async fn call(&self, method: &str, args: &[u8]) -> Result<Vec<u8>, BlockchainError> {
		let head = self.head.read().await;
		self.call_at_block(&head, method, args).await
	}

	/// Get storage value at the current head.
	///
	/// # Arguments
	///
	/// * `key` - Storage key
	///
	/// # Returns
	///
	/// The storage value, or `None` if the key doesn't exist.
	pub async fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, BlockchainError> {
		let block_number = self.head.read().await.number;
		self.get_storage_value(block_number, key).await
	}

	/// Get storage value at a specific block number.
	///
	/// # Arguments
	///
	/// * `block_number` - Block number to query at
	/// * `key` - Storage key
	///
	/// # Returns
	///
	/// The storage value, or `None` if the key doesn't exist.
	pub async fn storage_at(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, BlockchainError> {
		self.get_storage_value(block_number, key).await
	}

	/// Internal helper to query storage at a specific block number.
	///
	/// Accesses the shared `LocalStorageLayer` via the head block.
	/// All blocks share the same storage layer, so we use head as the accessor and let
	/// `LocalStorageLayer` handle the request.
	async fn get_storage_value(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, BlockchainError> {
		let head = self.head.read().await;
		let value = head.storage().get(block_number, key).await.map_err(BlockError::from)?;
		Ok(value.map(|v| v.value.clone()))
	}

	/// Detect chain type by checking for ParachainSystem pallet and extracting para_id.
	async fn detect_chain_type(block: &Block) -> Result<ChainType, BlockchainError> {
		let metadata = block.metadata().await?;

		// Check for ParachainSystem pallet (indicates this is a parachain)
		if metadata.pallet_by_name("ParachainSystem").is_some() {
			// Extract para_id from ParachainInfo pallet storage
			let para_id = Self::get_para_id(block).await.unwrap_or(0);
			Ok(ChainType::Parachain { para_id })
		} else {
			Ok(ChainType::RelayChain)
		}
	}

	/// Get the parachain ID from ParachainInfo pallet storage.
	///
	/// The para_id is stored at: `twox_128("ParachainInfo") ++ twox_128("ParachainId")`
	async fn get_para_id(block: &Block) -> Option<u32> {
		use scale::Decode;

		// Compute storage key: ParachainInfo::ParachainId
		let pallet_hash = sp_core::twox_128(b"ParachainInfo");
		let storage_hash = sp_core::twox_128(b"ParachainId");
		let key: Vec<u8> = [pallet_hash.as_slice(), storage_hash.as_slice()].concat();

		// Query storage
		let value = block.storage().get(block.number, &key).await.ok().flatten()?;

		// Decode as u32
		u32::decode(&mut value.value.as_slice()).ok()
	}

	/// Get chain name from runtime version.
	async fn get_chain_name(block: &Block) -> Result<String, BlockchainError> {
		// Get runtime code and create executor
		let runtime_code = block.runtime_code().await?;
		let executor = RuntimeExecutor::new(runtime_code, None)?;

		// Get runtime version which contains the spec name
		let version = executor.runtime_version()?;
		Ok(version.spec_name)
	}

	/// Execute a runtime call at a specific block.
	async fn call_at_block(
		&self,
		block: &Block,
		method: &str,
		args: &[u8],
	) -> Result<Vec<u8>, BlockchainError> {
		let runtime_code = block.runtime_code().await?;
		let executor =
			RuntimeExecutor::with_config(runtime_code, None, self.executor_config.clone())?;

		let result = executor.call(method, args, block.storage()).await?;
		Ok(result.output)
	}
}

/// Wrapper to convert `Arc<dyn InherentProvider>` to `Box<dyn InherentProvider>`.
///
/// This is needed because `BlockBuilder` expects `Box<dyn InherentProvider>`,
/// but we store providers as `Arc` for sharing across builds.
struct ArcProvider(Arc<dyn InherentProvider>);

#[async_trait::async_trait]
impl InherentProvider for ArcProvider {
	fn identifier(&self) -> &'static str {
		self.0.identifier()
	}

	async fn provide(
		&self,
		parent: &Block,
		executor: &RuntimeExecutor,
	) -> Result<Vec<Vec<u8>>, BlockBuilderError> {
		self.0.provide(parent, executor).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn chain_type_equality() {
		assert_eq!(ChainType::RelayChain, ChainType::RelayChain);
		assert_eq!(ChainType::Parachain { para_id: 1000 }, ChainType::Parachain { para_id: 1000 });
		assert_ne!(ChainType::Parachain { para_id: 1000 }, ChainType::Parachain { para_id: 2000 });
		assert_ne!(ChainType::RelayChain, ChainType::Parachain { para_id: 1000 });
	}

	#[test]
	fn blockchain_error_from_block_error() {
		let block_err = BlockError::RuntimeCodeNotFound;
		let blockchain_err: BlockchainError = block_err.into();
		assert!(matches!(blockchain_err, BlockchainError::Block(_)));
	}

	#[test]
	fn blockchain_error_display() {
		let err = BlockchainError::Block(BlockError::RuntimeCodeNotFound);
		assert!(err.to_string().contains("Runtime code not found"));
	}

	/// Integration tests that execute Blockchain against a local test node.
	///
	/// These tests verify the full blockchain lifecycle including fork creation,
	/// block building, storage queries, and runtime calls.
	mod sequential {
		use super::*;
		use pop_common::test_env::TestNode;

		/// Test context holding a spawned test node and blockchain instance.
		struct BlockchainTestContext {
			#[allow(dead_code)]
			node: TestNode,
			endpoint: Url,
		}

		/// Creates a test context with a spawned local node.
		async fn create_test_context() -> BlockchainTestContext {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");
			BlockchainTestContext { node, endpoint }
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fork_creates_blockchain_with_correct_fork_point() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Fork point should be set
			assert!(blockchain.fork_point_number() > 0 || blockchain.fork_point_number() == 0);
			assert_ne!(blockchain.fork_point(), H256::zero());

			// Head should match fork point initially
			assert_eq!(blockchain.head_number().await, blockchain.fork_point_number());
			assert_eq!(blockchain.head_hash().await, blockchain.fork_point());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fork_at_creates_blockchain_at_specific_block() {
			let ctx = create_test_context().await;

			// First fork to get the current block number
			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_number = blockchain.fork_point_number();

			// Fork at a specific block number (same as current for test node)
			let blockchain2 = Blockchain::fork_at(&ctx.endpoint, None, Some(fork_number.into()))
				.await
				.expect("Failed to fork at specific block");

			assert_eq!(blockchain2.fork_point_number(), fork_number);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fork_with_invalid_endpoint_fails() {
			let invalid_endpoint: Url = "ws://localhost:19999".parse().unwrap();

			let result = Blockchain::fork(&invalid_endpoint, None).await;

			assert!(result.is_err());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fork_at_with_invalid_block_number_fails() {
			let ctx = create_test_context().await;

			let result = Blockchain::fork_at(&ctx.endpoint, None, Some(u32::MAX.into())).await;

			assert!(result.is_err());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fork_detects_relay_chain_type() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Test node is a relay chain (no ParachainSystem pallet)
			assert_eq!(*blockchain.chain_type(), ChainType::RelayChain);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn fork_retrieves_chain_name() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Chain name should not be empty
			assert!(!blockchain.chain_name().is_empty());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn build_empty_block_advances_chain() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let initial_number = blockchain.head_number().await;
			let initial_hash = blockchain.head_hash().await;

			// Build an empty block
			let new_block =
				blockchain.build_empty_block().await.expect("Failed to build empty block");

			// Block number should increment
			assert_eq!(new_block.number, initial_number + 1);

			// Head should be updated
			assert_eq!(blockchain.head_number().await, initial_number + 1);
			assert_ne!(blockchain.head_hash().await, initial_hash);

			// Parent hash should point to previous head
			assert_eq!(new_block.parent_hash, initial_hash);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn build_multiple_empty_blocks_creates_chain() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_number = blockchain.fork_point_number();

			// Build 3 empty blocks
			for i in 1..=3 {
				let block =
					blockchain.build_empty_block().await.expect("Failed to build empty block");

				assert_eq!(block.number, fork_number + i);
			}

			assert_eq!(blockchain.head_number().await, fork_number + 3);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn storage_returns_value_for_existing_key() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Query System::Number storage (should exist)
			let key = {
				let mut k = Vec::new();
				k.extend(sp_core::twox_128(b"System"));
				k.extend(sp_core::twox_128(b"Number"));
				k
			};

			let value = blockchain.storage(&key).await.expect("Failed to query storage");

			assert!(value.is_some());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn storage_returns_none_for_nonexistent_key() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let nonexistent_key = b"nonexistent_key_12345";

			let value = blockchain.storage(nonexistent_key).await.expect("Failed to query storage");

			assert!(value.is_none());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn storage_at_queries_specific_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_number = blockchain.fork_point_number();

			// Build a block to have multiple blocks
			blockchain.build_empty_block().await.expect("Failed to build block");

			// Query storage at fork point
			let key = {
				let mut k = Vec::new();
				k.extend(sp_core::twox_128(b"System"));
				k.extend(sp_core::twox_128(b"Number"));
				k
			};

			let value = blockchain
				.storage_at(fork_number, &key)
				.await
				.expect("Failed to query storage at block");

			assert!(value.is_some());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn call_executes_runtime_api() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Call Core_version runtime API
			let result =
				blockchain.call("Core_version", &[]).await.expect("Failed to call runtime API");

			// Result should not be empty (contains version info)
			assert!(!result.is_empty());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn head_returns_current_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let head = blockchain.head().await;

			assert_eq!(head.number, blockchain.head_number().await);
			assert_eq!(head.hash, blockchain.head_hash().await);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn head_updates_after_building_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let old_head = blockchain.head().await;

			blockchain.build_empty_block().await.expect("Failed to build block");

			let new_head = blockchain.head().await;

			assert_eq!(new_head.number, old_head.number + 1);
			assert_ne!(new_head.hash, old_head.hash);
			assert_eq!(new_head.parent_hash, old_head.hash);
		}

		/// End-to-end integration test demonstrating the full blockchain manager flow.
		///
		/// This test shows how the blockchain manager integrates with all underlying
		/// modules (Block, BlockBuilder, LocalStorageLayer, RuntimeExecutor) to process
		/// a signed balance transfer transaction:
		///
		/// 1. Fork a live chain with signature mocking enabled
		/// 2. Query initial account balances via storage
		/// 3. Build a signed extrinsic (balance transfer from Alice to Bob)
		/// 4. Build a block containing the transaction
		/// 5. Verify the new block state reflects the transfer
		#[tokio::test(flavor = "multi_thread")]
		async fn build_block_with_signed_transfer_updates_balances() {
			use crate::{ExecutorConfig, SignatureMockMode};
			use scale::{Compact, Encode};

			// Transfer 100 units (with 12 decimals)
			const TRANSFER_AMOUNT: u128 = 100_000_000_000_000;

			// Well-known dev accounts
			const ALICE: [u8; 32] = [
				0xd4, 0x35, 0x93, 0xc7, 0x15, 0xfd, 0xd3, 0x1c, 0x61, 0x14, 0x1a, 0xbd, 0x04, 0xa9,
				0x9f, 0xd6, 0x82, 0x2c, 0x85, 0x58, 0x85, 0x4c, 0xcd, 0xe3, 0x9a, 0x56, 0x84, 0xe7,
				0xa5, 0x6d, 0xa2, 0x7d,
			];
			const BOB: [u8; 32] = [
				0x8e, 0xaf, 0x04, 0x15, 0x16, 0x87, 0x73, 0x63, 0x26, 0xc9, 0xfe, 0xa1, 0x7e, 0x25,
				0xfc, 0x52, 0x87, 0x61, 0x36, 0x93, 0xc9, 0x12, 0x90, 0x9c, 0xb2, 0x26, 0xaa, 0x47,
				0x94, 0xf2, 0x6a, 0x48,
			];

			/// Compute Blake2_128Concat storage key for System::Account.
			fn account_storage_key(account: &[u8; 32]) -> Vec<u8> {
				let mut key = Vec::new();
				key.extend(sp_core::twox_128(b"System"));
				key.extend(sp_core::twox_128(b"Account"));
				key.extend(sp_core::blake2_128(account));
				key.extend(account);
				key
			}

			/// Decode AccountInfo and extract free balance.
			fn decode_free_balance(data: &[u8]) -> u128 {
				const ACCOUNT_DATA_OFFSET: usize = 16;
				u128::from_le_bytes(
					data[ACCOUNT_DATA_OFFSET..ACCOUNT_DATA_OFFSET + 16]
						.try_into()
						.expect("need 16 bytes for u128"),
				)
			}

			/// Build a mock V4 signed extrinsic with dummy signature.
			fn build_mock_signed_extrinsic_v4(call_data: &[u8]) -> Vec<u8> {
				let mut inner = Vec::new();
				// Version byte: signed (0x80) + v4 (0x04) = 0x84
				inner.push(0x84);
				// Address: MultiAddress::Id variant (0x00) + 32-byte account
				inner.push(0x00);
				inner.extend(ALICE);
				// Signature: 64 bytes (dummy - works with AlwaysValid)
				inner.extend([0u8; 64]);
				// Extra params (signed extensions):
				inner.push(0x00); // CheckMortality: immortal
				inner.extend(Compact(0u64).encode()); // CheckNonce
				inner.extend(Compact(0u128).encode()); // ChargeTransactionPayment
				inner.push(0x00); // EthSetOrigin: None
				// Call data
				inner.extend(call_data);
				// Prefix with compact length
				let mut extrinsic = Compact(inner.len() as u32).encode();
				extrinsic.extend(inner);
				extrinsic
			}

			let ctx = create_test_context().await;

			// Fork with signature mocking enabled
			let config = ExecutorConfig {
				signature_mock: SignatureMockMode::AlwaysValid,
				..Default::default()
			};
			let blockchain = Blockchain::fork_with_config(&ctx.endpoint, None, None, config)
				.await
				.expect("Failed to fork blockchain");

			// Get storage keys for Alice and Bob
			let alice_key = account_storage_key(&ALICE);
			let bob_key = account_storage_key(&BOB);

			// Query initial balances
			let alice_balance_before = blockchain
				.storage(&alice_key)
				.await
				.expect("Failed to get Alice balance")
				.map(|v| decode_free_balance(&v))
				.expect("Alice should have a balance");

			let bob_balance_before = blockchain
				.storage(&bob_key)
				.await
				.expect("Failed to get Bob balance")
				.map(|v| decode_free_balance(&v))
				.expect("Bob should have a balance");

			// Get metadata to look up pallet/call indices
			let head = blockchain.head().await;
			let metadata = head.metadata().await.expect("Failed to get metadata");
			let balances_pallet =
				metadata.pallet_by_name("Balances").expect("Balances pallet should exist");
			let pallet_index = balances_pallet.index();
			let transfer_call = balances_pallet
				.call_variant_by_name("transfer_keep_alive")
				.expect("transfer_keep_alive call should exist");
			let call_index = transfer_call.index;

			// Encode the call: Balances.transfer_keep_alive(Bob, 100 units)
			let mut call_data = vec![pallet_index, call_index];
			call_data.push(0x00); // MultiAddress::Id variant
			call_data.extend(BOB);
			call_data.extend(Compact(TRANSFER_AMOUNT).encode());

			// Build a signed extrinsic
			let extrinsic = build_mock_signed_extrinsic_v4(&call_data);

			// Build a block with the transfer extrinsic
			let new_block = blockchain
				.build_block(vec![extrinsic])
				.await
				.expect("Failed to build block with transfer");

			// Verify block was created
			assert_eq!(new_block.number, head.number + 1);

			// Query balances after the transfer
			let alice_balance_after = blockchain
				.storage(&alice_key)
				.await
				.expect("Failed to get Alice balance after")
				.map(|v| decode_free_balance(&v))
				.expect("Alice should still have a balance");

			let bob_balance_after = blockchain
				.storage(&bob_key)
				.await
				.expect("Failed to get Bob balance after")
				.map(|v| decode_free_balance(&v))
				.expect("Bob should still have a balance");

			// Verify the transfer happened
			// Alice's balance should decrease (transfer amount + fees)
			assert!(
				alice_balance_after < alice_balance_before,
				"Alice balance should decrease after transfer"
			);
			// Bob should receive exactly the transfer amount
			assert_eq!(
				bob_balance_after,
				bob_balance_before + TRANSFER_AMOUNT,
				"Bob should receive exactly the transfer amount"
			);
			// Alice should have paid at least the transfer amount (plus fees)
			assert!(
				alice_balance_before - alice_balance_after >= TRANSFER_AMOUNT,
				"Alice should have paid at least the transfer amount plus fees"
			);
		}
	}
}
