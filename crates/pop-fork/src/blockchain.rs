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
	ExecutorError, ForkRpcClient, InherentProvider, RuntimeExecutor, StorageCache,
	builder::ApplyExtrinsicResult,
	create_next_header_with_slot, default_providers,
	strings::txpool::{runtime_api, transaction_source},
};
use scale::{Decode, Encode};
use std::{path::Path, sync::Arc};
use subxt::config::substrate::H256;
use tokio::sync::{RwLock, broadcast};
use url::Url;

pub type BlockBody = Vec<Vec<u8>>;

// Transaction validity types for decoding TaggedTransactionQueue_validate_transaction results.

/// Result of transaction validation.
///
/// Mirrors `sp_runtime::transaction_validity::TransactionValidity`.
#[derive(Debug, Clone, Decode)]
pub enum TransactionValidity {
	/// Transaction is valid.
	#[codec(index = 0)]
	Ok(ValidTransaction),
	/// Transaction is invalid.
	#[codec(index = 1)]
	Err(TransactionValidityError),
}

/// Information about a valid transaction.
#[derive(Debug, Clone, Decode)]
pub struct ValidTransaction {
	/// Priority of the transaction (higher = more likely to be included).
	pub priority: u64,
	/// Transaction dependencies (tags this tx requires).
	pub requires: Vec<Vec<u8>>,
	/// Tags this transaction provides.
	pub provides: Vec<Vec<u8>>,
	/// Longevity - how long this tx is valid (in blocks).
	pub longevity: u64,
	/// Whether this transaction should be propagated.
	pub propagate: bool,
}

/// Error when transaction validation fails.
#[derive(Debug, Clone, Decode)]
pub enum TransactionValidityError {
	/// Transaction is invalid (won't ever be valid).
	#[codec(index = 0)]
	Invalid(InvalidTransaction),
	/// Transaction validity is unknown (might become valid).
	#[codec(index = 1)]
	Unknown(UnknownTransaction),
}

/// Reasons a transaction is invalid.
#[derive(Debug, Clone, Decode)]
pub enum InvalidTransaction {
	/// General call failure.
	#[codec(index = 0)]
	Call,
	/// Payment failed (can't pay fees).
	#[codec(index = 1)]
	Payment,
	/// Future transaction (nonce too high).
	#[codec(index = 2)]
	Future,
	/// Stale transaction (nonce too low).
	#[codec(index = 3)]
	Stale,
	/// Bad mandatory inherent.
	#[codec(index = 4)]
	BadMandatory,
	/// Mandatory dispatch error.
	#[codec(index = 5)]
	MandatoryDispatch,
	/// Bad signature.
	#[codec(index = 6)]
	BadSigner,
	/// Custom error (runtime-specific).
	#[codec(index = 7)]
	Custom(u8),
}

/// Reasons transaction validity is unknown.
#[derive(Debug, Clone, Decode)]
pub enum UnknownTransaction {
	/// Can't lookup validity (dependencies missing).
	#[codec(index = 0)]
	CannotLookup,
	/// No unsigned validation handler.
	#[codec(index = 1)]
	NoUnsignedValidator,
	/// Custom unknown error.
	#[codec(index = 2)]
	Custom(u8),
}

impl TransactionValidityError {
	/// Get a human-readable reason for the error.
	pub fn reason(&self) -> String {
		match self {
			Self::Invalid(inv) => match inv {
				InvalidTransaction::Call => "Call failed".into(),
				InvalidTransaction::Payment => "Insufficient funds for fees".into(),
				InvalidTransaction::Future => "Nonce too high".into(),
				InvalidTransaction::Stale => "Nonce too low (already used)".into(),
				InvalidTransaction::BadMandatory => "Bad mandatory inherent".into(),
				InvalidTransaction::MandatoryDispatch => "Mandatory dispatch failed".into(),
				InvalidTransaction::BadSigner => "Invalid signature".into(),
				InvalidTransaction::Custom(code) => format!("Custom error: {code}"),
			},
			Self::Unknown(unk) => match unk {
				UnknownTransaction::CannotLookup => "Cannot lookup validity".into(),
				UnknownTransaction::NoUnsignedValidator => "No unsigned validator".into(),
				UnknownTransaction::Custom(code) => format!("Custom unknown: {code}"),
			},
		}
	}

	/// Check if this is an "unknown" error (might become valid later).
	pub fn is_unknown(&self) -> bool {
		matches!(self, Self::Unknown(_))
	}
}

/// Result of building a block, including information about extrinsic processing.
#[derive(Debug, Clone)]
pub struct BuildBlockResult {
	/// The newly built block.
	pub block: Block,
	/// Extrinsics that were successfully included.
	pub included: Vec<Vec<u8>>,
	/// Extrinsics that failed during apply and were dropped.
	pub failed: Vec<FailedExtrinsic>,
}

/// An extrinsic that failed during block building.
#[derive(Debug, Clone)]
pub struct FailedExtrinsic {
	/// The raw extrinsic bytes.
	pub extrinsic: Vec<u8>,
	/// Reason for failure.
	pub reason: String,
}

/// Capacity for the blockchain event broadcast channel.
const EVENT_CHANNEL_CAPACITY: usize = 256;

/// Events emitted by the blockchain when state changes.
///
/// Subscribe to these events via [`Blockchain::subscribe_events`] to receive
/// notifications when blocks are built.
#[derive(Debug, Clone)]
pub enum BlockchainEvent {
	/// A new block was built and is now the head.
	NewBlock {
		/// The new block's hash.
		hash: H256,
		/// The new block's number.
		number: u32,
		/// The parent block's hash.
		parent_hash: H256,
		/// The SCALE-encoded block header.
		header: Vec<u8>,
		/// Storage keys that were modified in this block.
		modified_keys: Vec<Vec<u8>>,
	},
}

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

	/// RPC endpoint URL for fetching remote data.
	endpoint: Url,

	/// Event broadcaster for subscription notifications.
	///
	/// Subscriptions receive events through receivers obtained via
	/// [`subscribe_events`](Blockchain::subscribe_events).
	event_tx: broadcast::Sender<BlockchainEvent>,
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
	/// use std::path::Path;
	/// use url::Url;
	///
	/// let endpoint: Url = "wss://rpc.polkadot.io".parse()?;
	///
	/// // With in-memory cache
	/// let blockchain = Blockchain::fork(&endpoint, None).await?;
	///
	/// // With persistent cache
	/// let blockchain = Blockchain::fork(&endpoint, Some(Path::new("./cache.sqlite"))).await?;
	/// ```
	pub async fn fork(
		endpoint: &Url,
		cache_path: Option<&Path>,
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
		cache_path: Option<&Path>,
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
		cache_path: Option<&Path>,
		fork_point: Option<BlockForkPoint>,
		executor_config: ExecutorConfig,
	) -> Result<Arc<Self>, BlockchainError> {
		// Create storage cache
		let cache = StorageCache::open(cache_path).await?;

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

		// Create event broadcast channel
		let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);

		Ok(Arc::new(Self {
			head: RwLock::new(fork_block),
			inherent_providers,
			chain_name,
			chain_type,
			fork_point_hash,
			fork_point_number,
			executor_config,
			endpoint: endpoint.clone(),
			event_tx,
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

	/// Get the RPC endpoint URL.
	pub fn endpoint(&self) -> &Url {
		&self.endpoint
	}

	/// Subscribe to blockchain events.
	///
	/// Returns a receiver that will get events when blocks are built.
	/// Use this for implementing reactive RPC subscriptions.
	///
	/// # Example
	///
	/// ```ignore
	/// let mut receiver = blockchain.subscribe_events();
	/// tokio::spawn(async move {
	///     while let Ok(event) = receiver.recv().await {
	///         match event {
	///             BlockchainEvent::NewBlock { hash, number, .. } => {
	///                 println!("New block #{} ({:?})", number, hash);
	///             }
	///         }
	///     }
	/// });
	/// ```
	pub fn subscribe_events(&self) -> broadcast::Receiver<BlockchainEvent> {
		self.event_tx.subscribe()
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

	/// Get block body (extrinsics) by hash.
	///
	/// This method searches for the block in three places:
	/// 1. The current head block
	/// 2. Locally-built blocks (traversing the parent chain)
	/// 3. The remote chain (for blocks at or before the fork point)
	///
	/// # Arguments
	///
	/// * `hash` - The block hash to query
	///
	/// # Returns
	///
	/// The block's extrinsics as raw bytes, or `None` if the block is not found.
	pub async fn block_body(&self, hash: H256) -> Result<Option<BlockBody>, BlockchainError> {
		// First, check if it matches any locally-built block (but not the fork point,
		// which has empty extrinsics since we don't fetch them during fork)
		let head = self.head.read().await;

		// Traverse the parent chain to find the block
		let mut current: Option<&Block> = Some(&head);
		while let Some(block) = current {
			if block.hash == hash {
				// If this is the fork point (no parent), we need to fetch from remote
				// because fork point's extrinsics are not stored locally
				if block.parent.is_none() {
					break; // Fall through to remote fetch
				}
				return Ok(Some(block.extrinsics.clone()));
			}
			current = block.parent.as_deref();
		}

		// Not found locally or is fork point - fetch from remote RPC
		let rpc = ForkRpcClient::connect(&self.endpoint).await.map_err(BlockError::from)?;

		match rpc.block_by_hash(hash).await.map_err(BlockError::from)? {
			Some(block) => {
				// Convert extrinsics to raw bytes
				let extrinsics: BlockBody =
					block.extrinsics.into_iter().map(|ext| ext.0.to_vec()).collect();
				Ok(Some(extrinsics))
			},
			None => Ok(None),
		}
	}

	/// Get block header by hash.
	///
	/// This method searches for the block in three places:
	/// 1. Locally-built blocks (traversing the parent chain)
	/// 2. The fork point block
	/// 3. The remote chain (for blocks at or before the fork point)
	///
	/// # Arguments
	///
	/// * `hash` - The block hash to query
	///
	/// # Returns
	///
	/// The SCALE-encoded block header, or `None` if the block is not found.
	pub async fn block_header(&self, hash: H256) -> Result<Option<Vec<u8>>, BlockchainError> {
		let head = self.head.read().await;

		// Traverse the parent chain to find the block
		let mut current: Option<&Block> = Some(&head);
		while let Some(block) = current {
			if block.hash == hash {
				return Ok(Some(block.header.clone()));
			}
			current = block.parent.as_deref();
		}

		// Not found locally  - fetch from remote RPC
		let rpc = ForkRpcClient::connect(&self.endpoint).await.map_err(BlockError::from)?;

		// Use existing header() method - errors (block not found) are converted to None
		match rpc.header(hash).await {
			Ok(header) => Ok(Some(header.encode())),
			Err(_) => Ok(None), // Block not found on remote
		}
	}

	/// Get block hash by block number.
	///
	/// This method searches for the block in three places:
	/// 1. Locally-built blocks (traversing the parent chain from head)
	/// 2. The fork point block
	/// 3. The remote chain (for blocks before the fork point)
	///
	/// # Arguments
	///
	/// * `block_number` - The block number to query
	///
	/// # Returns
	///
	/// The block hash, or `None` if the block number doesn't exist.
	pub async fn block_hash_at(&self, block_number: u32) -> Result<Option<H256>, BlockchainError> {
		// Check if block number is within our local chain range
		let head = self.head.read().await;

		if head.number < block_number {
			return Ok(None);
		}

		// Traverse the parent chain to find the block by number
		let mut current: Option<&Block> = Some(&head);
		while let Some(block) = current {
			if block.number == block_number {
				return Ok(Some(block.hash));
			}

			if block.parent.is_none() {
				break;
			}

			current = block.parent.as_deref();
		}

		// Block number is before our fork point or not found - fetch from remote RPC
		let rpc = ForkRpcClient::connect(&self.endpoint).await.map_err(BlockError::from)?;
		Ok(rpc.block_hash_at(block_number).await.map_err(BlockError::from)?)
	}

	/// Get block number by block hash.
	///
	/// This method searches for the block in two places:
	/// 1. Locally-built blocks (traversing the parent chain from head)
	/// 2. The remote chain (for blocks at or before the fork point)
	///
	/// # Arguments
	///
	/// * `hash` - The block hash to query
	///
	/// # Returns
	///
	/// The block number, or `None` if the block hash doesn't exist.
	pub async fn block_number_by_hash(&self, hash: H256) -> Result<Option<u32>, BlockchainError> {
		// Traverse local chain to find block by hash
		let head = self.head.read().await;
		let mut current: Option<&Block> = Some(&head);
		while let Some(block) = current {
			if block.hash == hash {
				return Ok(Some(block.number));
			}
			current = block.parent.as_deref();
		}

		// Not found locally - check if it exists on remote chain
		let rpc = ForkRpcClient::connect(&self.endpoint).await.map_err(BlockError::from)?;
		match rpc.block_by_hash(hash).await.map_err(BlockError::from)? {
			Some(block) => Ok(Some(block.header.number)),
			None => Ok(None),
		}
	}

	/// Get parent hash of a block by its hash.
	///
	/// This method searches for the block in two places:
	/// 1. Locally-built blocks (traversing the parent chain from head)
	/// 2. The remote chain (for blocks at or before the fork point)
	///
	/// # Arguments
	///
	/// * `hash` - The block hash to query
	///
	/// # Returns
	///
	/// The parent block hash, or `None` if the block hash doesn't exist.
	pub async fn block_parent_hash(&self, hash: H256) -> Result<Option<H256>, BlockchainError> {
		// Traverse local chain to find block by hash
		let head = self.head.read().await;
		let mut current: Option<&Block> = Some(&head);
		while let Some(block) = current {
			if block.hash == hash {
				return Ok(Some(block.parent_hash));
			}
			current = block.parent.as_deref();
		}

		// Not found locally - check if it exists on remote chain
		let rpc = ForkRpcClient::connect(&self.endpoint).await.map_err(BlockError::from)?;
		match rpc.block_by_hash(hash).await.map_err(BlockError::from)? {
			Some(block) => Ok(Some(block.header.parent_hash)),
			None => Ok(None),
		}
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
	/// A [`BuildBlockResult`] containing the newly created block and information
	/// about which extrinsics were successfully included vs which failed.
	///
	/// # Example
	///
	/// ```ignore
	/// let extrinsic = /* create signed extrinsic */;
	/// let result = blockchain.build_block(vec![extrinsic]).await?;
	/// println!("New block: #{} ({:?})", result.block.number, result.block.hash);
	/// println!("Included: {}, Failed: {}", result.included.len(), result.failed.len());
	/// ```
	pub async fn build_block(
		&self,
		extrinsics: BlockBody,
	) -> Result<BuildBlockResult, BlockchainError> {
		// PHASE 1: Prepare (read lock only) - get state needed for building
		let (parent_block, runtime_code, parent_hash) = {
			let head = self.head.read().await;
			let runtime_code = head.runtime_code().await?;
			let parent_hash = head.hash;
			(head.clone(), runtime_code, parent_hash)
		}; // Read lock released here

		// PHASE 2: Build (no lock held) - allows concurrent reads
		// Create executor with current runtime and configured settings
		let executor =
			RuntimeExecutor::with_config(runtime_code, None, self.executor_config.clone())?;

		// Create header for new block with automatic slot digest injection
		let header = create_next_header_with_slot(&parent_block, &executor, vec![]).await?;

		// Convert Arc providers to Box for BlockBuilder
		let providers: Vec<Box<dyn InherentProvider>> = self
			.inherent_providers
			.iter()
			.map(|p| Box::new(ArcProvider(Arc::clone(p))) as Box<dyn InherentProvider>)
			.collect();

		// Create block builder
		let mut builder = BlockBuilder::new(parent_block, executor, header, providers);

		// Initialize block
		builder.initialize().await?;

		// Apply inherents
		builder.apply_inherents().await?;

		// Track included and failed extrinsics
		let mut included = Vec::new();
		let mut failed = Vec::new();

		// Apply user extrinsics
		for extrinsic in extrinsics {
			match builder.apply_extrinsic(extrinsic.clone()).await? {
				ApplyExtrinsicResult::Success { .. } => {
					included.push(extrinsic);
				},
				ApplyExtrinsicResult::DispatchFailed { error } => {
					failed.push(FailedExtrinsic { extrinsic, reason: error });
				},
			}
		}

		// Finalize and get new block
		let new_block = builder.finalize().await?;

		// PHASE 3: Commit (brief write lock) - update head
		{
			let mut head = self.head.write().await;
			// Verify parent hasn't changed (optimistic concurrency check)
			if head.hash != parent_hash {
				return Err(BlockchainError::Block(BlockError::ConcurrentBlockBuild));
			}
			*head = new_block.clone();
		} // Write lock released here before event emission

		// Get modified keys from storage diff
		let modified_keys: Vec<Vec<u8>> = new_block
			.storage()
			.diff()
			.map(|diff| diff.into_iter().map(|(k, _)| k).collect())
			.unwrap_or_default();

		// Emit event AFTER releasing lock (ignore errors - no subscribers is OK)
		let _ = self.event_tx.send(BlockchainEvent::NewBlock {
			hash: new_block.hash,
			number: new_block.number,
			parent_hash: new_block.parent_hash,
			header: new_block.header.clone(),
			modified_keys,
		});

		Ok(BuildBlockResult { block: new_block, included, failed })
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
		self.build_block(vec![]).await.map(|result| result.block)
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
		let head_hash = self.head_hash().await;
		self.call_at_block(head_hash, method, args)
			.await
			.map(|result| result.expect("head_hash always exists; qed;"))
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
		Ok(value.and_then(|v| v.value.clone()))
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

		value.value.as_ref().map(|value| u32::decode(&mut value.as_slice()).ok())?
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

	/// Execute a runtime call at a specific block hash.
	///
	/// # Arguments
	///
	/// * `hash` - The block hash to execute at
	/// * `method` - Runtime API method name (e.g., "Core_version")
	/// * `args` - SCALE-encoded arguments
	///
	/// # Returns
	///
	/// * `Ok(Some(result))` - Call succeeded at the specified block
	/// * `Ok(None)` - Block hash not found
	/// * `Err(_)` - Call failed (runtime error, storage error, etc.)
	pub async fn call_at_block(
		&self,
		hash: H256,
		method: &str,
		args: &[u8],
	) -> Result<Option<Vec<u8>>, BlockchainError> {
		// Find block: search fork history or create mocked block for historical
		let block = self.find_or_create_block_for_call(hash).await?;

		let Some(block) = block else {
			return Ok(None); // Block not found
		};

		// Execute call on the found/created block
		let runtime_code = block.runtime_code().await?;
		let executor =
			RuntimeExecutor::with_config(runtime_code, None, self.executor_config.clone())?;
		let result = executor.call(method, args, block.storage()).await?;
		Ok(Some(result.output))
	}

	/// Validate an extrinsic before pool submission.
	///
	/// Calls `TaggedTransactionQueue_validate_transaction` runtime API to check
	/// if the extrinsic is valid for inclusion in a future block.
	///
	/// # Arguments
	///
	/// * `extrinsic` - The encoded extrinsic to validate
	///
	/// # Returns
	///
	/// * `Ok(ValidTransaction)` - Transaction is valid with priority/dependency info
	/// * `Err(TransactionValidityError)` - Transaction is invalid or unknown
	///
	/// # Example
	///
	/// ```ignore
	/// match blockchain.validate_extrinsic(&extrinsic_bytes).await {
	///     Ok(valid) => println!("Valid with priority {}", valid.priority),
	///     Err(TransactionValidityError::Invalid(inv)) => {
	///         println!("Invalid: {:?}", inv);
	///     }
	///     Err(TransactionValidityError::Unknown(unk)) => {
	///         println!("Unknown validity: {:?}", unk);
	///     }
	/// }
	/// ```
	pub async fn validate_extrinsic(
		&self,
		extrinsic: &[u8],
	) -> Result<ValidTransaction, TransactionValidityError> {
		let head = self.head.read().await;

		// Build args: (source, extrinsic, block_hash)
		// source = External (0x02) - transaction comes from outside
		// Note: Raw concatenation matches how the runtime expects the input.
		// The extrinsic is passed as-is since it already includes its SCALE encoding.
		let mut args = Vec::with_capacity(1 + extrinsic.len() + 32);
		args.push(transaction_source::EXTERNAL);
		args.extend(extrinsic);
		args.extend(head.hash.as_bytes());

		// Get runtime code and create executor
		let runtime_code = head
			.runtime_code()
			.await
			.map_err(|_| TransactionValidityError::Unknown(UnknownTransaction::CannotLookup))?;

		let executor =
			RuntimeExecutor::with_config(runtime_code, None, self.executor_config.clone())
				.map_err(|_| TransactionValidityError::Unknown(UnknownTransaction::CannotLookup))?;

		// Call runtime API
		let result = executor
			.call(runtime_api::TAGGED_TRANSACTION_QUEUE_VALIDATE, &args, head.storage())
			.await
			.map_err(|_| TransactionValidityError::Unknown(UnknownTransaction::CannotLookup))?;

		// Decode result
		let validity = TransactionValidity::decode(&mut result.output.as_slice())
			.map_err(|_| TransactionValidityError::Unknown(UnknownTransaction::CannotLookup))?;

		match validity {
			TransactionValidity::Ok(valid) => Ok(valid),
			TransactionValidity::Err(err) => Err(err),
		}
	}

	/// Find a block by hash in fork history, or create a mocked block for historical execution.
	///
	/// Returns:
	/// - `Some(block)` if found in fork history or exists on remote chain
	/// - `None` if block doesn't exist anywhere
	async fn find_or_create_block_for_call(
		&self,
		hash: H256,
	) -> Result<Option<Block>, BlockchainError> {
		let head = self.head.read().await;

		// Search fork history
		let mut current: Option<&Block> = Some(&head);
		while let Some(block) = current {
			if block.hash == hash {
				return Ok(Some(block.clone()));
			}
			// Stop at fork point - anything before this is on remote chain
			if block.parent.is_none() {
				break;
			}
			current = block.parent.as_deref();
		}

		// Not in fork history - check if block exists on remote chain
		let rpc = ForkRpcClient::connect(&self.endpoint).await.map_err(BlockError::from)?;

		let remote_block = match rpc.block_by_hash(hash).await.map_err(BlockError::from)? {
			Some(block) => block,
			None => return Ok(None), // Block doesn't exist anywhere
		};

		// Extract block number from header
		let block_number = remote_block.header.number;

		// Block exists on remote - create mocked block with real hash and number
		// Storage layer delegates to remote for historical data
		Ok(Some(Block::mocked_for_call(hash, block_number, head.storage().clone())))
	}

	/// Set storage value at the current head (for testing purposes).
	///
	/// This method allows tests to manually set storage values to create
	/// differences between blocks for testing storage diff functionality.
	///
	/// # Arguments
	///
	/// * `key` - Storage key
	/// * `value` - Value to set, or `None` to delete
	///
	/// # Returns
	///
	/// `Ok(())` on success, or an error if storage modification fails.
	#[cfg(test)]
	pub async fn set_storage_for_testing(&self, key: &[u8], value: Option<&[u8]>) {
		let mut head = self.head.write().await;
		head.storage_mut().set(key, value).unwrap();
	}

	/// Clear all locally tracked storage data from the cache.
	///
	/// This removes all key-value pairs that were created during block building
	/// (stored in the `local_keys` and `local_values` tables). Remote chain data
	/// that was fetched and cached remains intact.
	///
	/// Call this during shutdown to clean up local storage state.
	///
	/// # Returns
	///
	/// `Ok(())` on success, or a cache error if the operation fails.
	pub async fn clear_local_storage(&self) -> Result<(), CacheError> {
		let head = self.head.read().await;
		head.storage().cache().clear_local_storage().await
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

		/// Transfer amount: 100 units (with 12 decimals)
		const TRANSFER_AMOUNT: u128 = 100_000_000_000_000;

		/// Well-known dev account: Alice
		const ALICE: [u8; 32] = [
			0xd4, 0x35, 0x93, 0xc7, 0x15, 0xfd, 0xd3, 0x1c, 0x61, 0x14, 0x1a, 0xbd, 0x04, 0xa9,
			0x9f, 0xd6, 0x82, 0x2c, 0x85, 0x58, 0x85, 0x4c, 0xcd, 0xe3, 0x9a, 0x56, 0x84, 0xe7,
			0xa5, 0x6d, 0xa2, 0x7d,
		];

		/// Well-known dev account: Bob
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

		/// Build a mock V4 signed extrinsic with dummy signature (from Alice).
		fn build_mock_signed_extrinsic_v4(call_data: &[u8]) -> Vec<u8> {
			use scale::{Compact, Encode};
			let mut inner = Vec::new();
			inner.push(0x84); // Version: signed (0x80) + v4 (0x04)
			inner.push(0x00); // MultiAddress::Id variant
			inner.extend(ALICE);
			inner.extend([0u8; 64]); // Dummy signature (works with AlwaysValid)
			inner.push(0x00); // CheckMortality: immortal
			inner.extend(Compact(0u64).encode()); // CheckNonce
			inner.extend(Compact(0u128).encode()); // ChargeTransactionPayment
			inner.push(0x00); // EthSetOrigin: None
			inner.extend(call_data);
			let mut extrinsic = Compact(inner.len() as u32).encode();
			extrinsic.extend(inner);
			extrinsic
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

			// Get head block for metadata and capture block number before building
			let head = blockchain.head().await;
			let head_number_before = head.number;
			let metadata = head.metadata().await.expect("Failed to get metadata");

			// Query initial balances at the current head block
			let alice_balance_before = blockchain
				.storage_at(head_number_before, &alice_key)
				.await
				.expect("Failed to get Alice balance")
				.map(|v| decode_free_balance(&v))
				.expect("Alice should have a balance");

			let bob_balance_before = blockchain
				.storage_at(head_number_before, &bob_key)
				.await
				.expect("Failed to get Bob balance")
				.map(|v| decode_free_balance(&v))
				.expect("Bob should have a balance");
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
			let result = blockchain
				.build_block(vec![extrinsic])
				.await
				.expect("Failed to build block with transfer");

			let new_block = result.block;

			// Verify block was created
			assert_eq!(new_block.number, head_number_before + 1);

			// Query balances after the transfer at the new block
			let alice_balance_after = blockchain
				.storage_at(new_block.number, &alice_key)
				.await
				.expect("Failed to get Alice balance after")
				.map(|v| decode_free_balance(&v))
				.expect("Alice should still have a balance");

			let bob_balance_after = blockchain
				.storage_at(new_block.number, &bob_key)
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

		#[tokio::test(flavor = "multi_thread")]
		async fn block_body_returns_extrinsics_for_head() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build a block so we have extrinsics (inherents)
			let block = blockchain.build_empty_block().await.expect("Failed to build block");

			// Query body for head hash
			let body = blockchain.block_body(block.hash).await.expect("Failed to get block body");

			assert!(body.is_some(), "Should return body for head hash");
			// Should have inherent extrinsics
			let extrinsics = body.unwrap();
			assert!(!extrinsics.is_empty(), "Built block should have inherent extrinsics");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_body_returns_extrinsics_for_parent_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build two blocks
			let block1 = blockchain.build_empty_block().await.expect("Failed to build block 1");
			let _block2 = blockchain.build_empty_block().await.expect("Failed to build block 2");

			// Query body for the first built block (parent of head)
			let body = blockchain.block_body(block1.hash).await.expect("Failed to get block body");

			assert!(body.is_some(), "Should return body for parent block");
			let extrinsics = body.unwrap();
			assert!(!extrinsics.is_empty(), "Parent block should have inherent extrinsics");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_body_returns_extrinsics_for_fork_point_from_remote() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_point_hash = blockchain.fork_point();

			// Query body for fork point (should fetch from remote)
			let body =
				blockchain.block_body(fork_point_hash).await.expect("Failed to get block body");

			// Fork point exists on remote chain, so body should be Some
			assert!(body.is_some(), "Should return body for fork point from remote");
			assert!(!body.unwrap().is_empty(), "Should contain body");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_body_returns_none_for_unknown_hash() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Use a fabricated hash that doesn't exist
			let unknown_hash = H256::from([0xde; 32]);

			let body =
				blockchain.block_body(unknown_hash).await.expect("Failed to query block body");

			assert!(body.is_none(), "Should return None for unknown hash");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_header_returns_header_for_head() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build a block so we have a locally-built header
			let block = blockchain.build_empty_block().await.expect("Failed to build block");

			// Query header for head hash
			let header =
				blockchain.block_header(block.hash).await.expect("Failed to get block header");

			assert!(header.is_some(), "Should return header for head hash");
			// Header should not be empty
			let header_bytes = header.unwrap();
			assert!(!header_bytes.is_empty(), "Built block should have a header");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_header_returns_header_for_different_blocks() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build two blocks
			let block1 = blockchain.build_empty_block().await.expect("Failed to build block 1");
			let block2 = blockchain.build_empty_block().await.expect("Failed to build block 2");

			let header_1 = blockchain
				.block_header(block1.hash)
				.await
				.expect("Failed to get block header")
				.unwrap();
			let header_2 = blockchain
				.block_header(block2.hash)
				.await
				.expect("Failed to get block header")
				.unwrap();

			assert_ne!(header_1, header_2);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_header_returns_header_for_fork_point() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_point_hash = blockchain.fork_point();

			// Query header for fork point (should fetch from remote)
			let header = blockchain
				.block_header(fork_point_hash)
				.await
				.expect("Failed to get block header");

			// Fork point exists on remote chain, so header should be Some
			assert!(header.is_some(), "Should return header for fork point from remote");
			assert!(!header.unwrap().is_empty(), "Should contain header");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_header_returns_none_for_unknown_hash() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Use a fabricated hash that doesn't exist
			let unknown_hash = H256::from([0xde; 32]);

			let header = blockchain
				.block_header(unknown_hash)
				.await
				.expect("Failed to query block header");

			assert!(header.is_none(), "Should return None for unknown hash");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_header_returns_header_for_historical_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_number = blockchain.fork_point_number();

			// Only test if fork point is > 0 (has blocks before it)
			if fork_number > 0 {
				// Get the hash of a block before the fork point
				let historical_hash = blockchain
					.block_hash_at(fork_number - 1)
					.await
					.expect("Failed to get historical hash")
					.expect("Historical block should exist");

				// Query header for historical block (before fork point, on remote chain)
				let header = blockchain
					.block_header(historical_hash)
					.await
					.expect("Failed to get block header");

				assert!(header.is_some(), "Should return header for historical block");
				assert!(!header.unwrap().is_empty(), "Historical block should have a header");
			}
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_hash_at_returns_hash_for_head() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build a block
			let block = blockchain.build_empty_block().await.expect("Failed to build block");

			// Query hash for head block number
			let hash =
				blockchain.block_hash_at(block.number).await.expect("Failed to get block hash");

			assert!(hash.is_some(), "Should return hash for head block number");
			assert_eq!(hash.unwrap(), block.hash, "Hash should match head block hash");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_hash_at_returns_hash_for_parent_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build two blocks
			let block1 = blockchain.build_empty_block().await.expect("Failed to build block 1");
			let _block2 = blockchain.build_empty_block().await.expect("Failed to build block 2");

			// Query hash for the first built block
			let hash =
				blockchain.block_hash_at(block1.number).await.expect("Failed to get block hash");

			assert!(hash.is_some(), "Should return hash for parent block number");
			assert_eq!(hash.unwrap(), block1.hash, "Hash should match first block hash");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_hash_at_returns_hash_for_fork_point() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_point_number = blockchain.fork_point_number();
			let fork_point_hash = blockchain.fork_point();

			// Query hash for fork point
			let hash = blockchain
				.block_hash_at(fork_point_number)
				.await
				.expect("Failed to get block hash");

			assert!(hash.is_some(), "Should return hash for fork point");
			assert_eq!(hash.unwrap(), fork_point_hash, "Hash should match fork point hash");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_hash_at_returns_hash_for_block_before_fork_point() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_point_number = blockchain.fork_point_number();

			// Only test if fork point is > 0 (has blocks before it)
			if fork_point_number > 0 {
				let hash = blockchain
					.block_hash_at(fork_point_number - 1)
					.await
					.expect("Failed to get block hash");

				assert!(hash.is_some(), "Should return hash for block before fork point");
			}
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_hash_at_returns_none_for_future_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let head_number = blockchain.head_number().await;

			// Query a block number that doesn't exist yet
			let hash = blockchain
				.block_hash_at(head_number + 100)
				.await
				.expect("Failed to query block hash");

			assert!(hash.is_none(), "Should return None for future block number");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_number_by_hash_returns_number_for_head() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build a block
			let block = blockchain.build_empty_block().await.unwrap();

			// Query number by hash
			let number = blockchain
				.block_number_by_hash(block.hash)
				.await
				.expect("Failed to query block number");

			assert_eq!(number, Some(block.number));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_number_by_hash_returns_number_for_parent() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build two blocks
			let block1 = blockchain.build_empty_block().await.unwrap();
			let _block2 = blockchain.build_empty_block().await.unwrap();

			// Query number for first block
			let number = blockchain
				.block_number_by_hash(block1.hash)
				.await
				.expect("Failed to query block number");

			assert_eq!(number, Some(block1.number));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_number_by_hash_returns_number_for_fork_point() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_hash = blockchain.fork_point();
			let fork_number = blockchain.fork_point_number();

			let number = blockchain
				.block_number_by_hash(fork_hash)
				.await
				.expect("Failed to query block number");

			assert_eq!(number, Some(fork_number));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_number_by_hash_returns_none_for_unknown() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let unknown_hash = H256::from_slice(&[0u8; 32]);
			let number = blockchain
				.block_number_by_hash(unknown_hash)
				.await
				.expect("Failed to query block number");

			assert!(number.is_none());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn block_number_by_hash_returns_number_for_historical_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Get a block before the fork point (if available)
			let fork_number = blockchain.fork_point_number();
			if fork_number > 0 {
				let historical_hash = blockchain
					.block_hash_at(fork_number - 1)
					.await
					.expect("Failed to query block hash")
					.expect("Block should exist");

				let number = blockchain
					.block_number_by_hash(historical_hash)
					.await
					.expect("Failed to query block number");

				assert_eq!(number, Some(fork_number - 1));
			}
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn call_at_block_executes_at_head() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			blockchain.build_empty_block().await.unwrap();
			blockchain.build_empty_block().await.unwrap();
			blockchain.build_empty_block().await.unwrap();

			let head_hash = blockchain.head_hash().await;

			// Call Core_version at head hash
			let result = blockchain
				.call_at_block(head_hash, "Core_version", &[])
				.await
				.expect("Failed to call runtime API");

			assert!(result.is_some(), "Should return result for head hash");
			assert!(!result.unwrap().is_empty(), "Result should not be empty");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn call_at_block_executes_at_fork_point() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			blockchain.build_empty_block().await.unwrap();
			blockchain.build_empty_block().await.unwrap();
			blockchain.build_empty_block().await.unwrap();

			let fork_hash = blockchain.fork_point();

			// Call Core_version at fork point
			let result = blockchain
				.call_at_block(fork_hash, "Core_version", &[])
				.await
				.expect("Failed to call runtime API");

			assert!(result.is_some(), "Should return result for fork point hash");
			assert!(!result.unwrap().is_empty(), "Result should not be empty");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn call_at_block_executes_at_parent_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Build two blocks
			let block1 = blockchain.build_empty_block().await.expect("Failed to build block 1");
			let _block2 = blockchain.build_empty_block().await.expect("Failed to build block 2");

			// Call at the first built block (parent of head)
			let result = blockchain
				.call_at_block(block1.hash, "Core_version", &[])
				.await
				.expect("Failed to call runtime API");

			assert!(result.is_some(), "Should return result for parent block hash");
			assert!(!result.unwrap().is_empty(), "Result should not be empty");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn call_at_block_returns_none_for_unknown_hash() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Use a fabricated hash that doesn't exist
			let unknown_hash = H256::from([0xde; 32]);

			let result = blockchain
				.call_at_block(unknown_hash, "Core_version", &[])
				.await
				.expect("Failed to query");

			assert!(result.is_none(), "Should return None for unknown hash");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn call_at_block_executes_at_historical_block() {
			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			let fork_number = blockchain.fork_point_number();

			// Only test if fork point is > 0 (has blocks before it)
			if fork_number > 0 {
				// Get the hash of a block before the fork point
				let historical_hash = blockchain
					.block_hash_at(fork_number - 1)
					.await
					.expect("Failed to get historical hash")
					.expect("Historical block should exist");

				// Call at historical block (before fork point, on remote chain)
				let result = blockchain
					.call_at_block(historical_hash, "Core_version", &[])
					.await
					.expect("Failed to call runtime API");

				assert!(result.is_some(), "Should return result for historical block");
				assert!(!result.unwrap().is_empty(), "Result should not be empty");
			}
		}

		/// Verifies that calling `Core_initialize_block` via `call_at_block` does NOT
		/// persist storage changes.
		///
		/// `Core_initialize_block` writes to `System::Number` and other storage keys during
		/// block initialization. This test verifies those changes are discarded after the call.
		#[tokio::test(flavor = "multi_thread")]
		async fn call_at_block_does_not_persist_storage() {
			use crate::{DigestItem, consensus_engine, create_next_header};

			let ctx = create_test_context().await;

			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Get head block info
			let head = blockchain.head().await;
			let head_hash = head.hash;
			let head_number = head.number;

			// System::Number storage key = twox128("System") ++ twox128("Number")
			let system_number_key: Vec<u8> =
				[sp_core::twox_128(b"System").as_slice(), sp_core::twox_128(b"Number").as_slice()]
					.concat();

			// Query System::Number BEFORE
			let number_before = blockchain
				.storage(&system_number_key)
				.await
				.expect("Failed to get System::Number")
				.map(|v| {
					u32::from_le_bytes(v.try_into().expect("System::Number should be 4 bytes"))
				})
				.expect("System::Number should exist");

			// Build header for the next block using the crate's helper
			let header = create_next_header(
				&head,
				vec![DigestItem::PreRuntime(consensus_engine::AURA, 0u64.to_le_bytes().to_vec())],
			);

			// Call Core_initialize_block - this WOULD write System::Number = head_number + 1
			let result = blockchain
				.call_at_block(head_hash, "Core_initialize_block", &header)
				.await
				.expect("Core_initialize_block call failed");
			assert!(result.is_some(), "Block should exist");

			// Query System::Number AFTER - should be UNCHANGED
			let number_after = blockchain
				.storage(&system_number_key)
				.await
				.expect("Failed to get System::Number after")
				.map(|v| {
					u32::from_le_bytes(v.try_into().expect("System::Number should be 4 bytes"))
				})
				.expect("System::Number should still exist");

			assert_eq!(
				number_before,
				number_after,
				"System::Number should NOT be modified by call_at_block. \
				 Before: {}, After: {} (would have been {} if persisted)",
				number_before,
				number_after,
				head_number + 1
			);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn validate_extrinsic_accepts_valid_transfer() {
			use crate::{ExecutorConfig, SignatureMockMode};
			use scale::{Compact, Encode};

			let ctx = create_test_context().await;
			let config = ExecutorConfig {
				signature_mock: SignatureMockMode::AlwaysValid,
				..Default::default()
			};
			let blockchain = Blockchain::fork_with_config(&ctx.endpoint, None, None, config)
				.await
				.expect("Failed to fork blockchain");

			// Build a valid transfer extrinsic
			let head = blockchain.head().await;
			let metadata = head.metadata().await.expect("Failed to get metadata");

			let balances_pallet = metadata.pallet_by_name("Balances").expect("Balances pallet");
			let pallet_index = balances_pallet.index();
			let transfer_call = balances_pallet
				.call_variant_by_name("transfer_keep_alive")
				.expect("transfer_keep_alive");
			let call_index = transfer_call.index;

			let mut call_data = vec![pallet_index, call_index];
			call_data.push(0x00); // MultiAddress::Id
			call_data.extend(BOB);
			call_data.extend(Compact(TRANSFER_AMOUNT).encode());

			let extrinsic = build_mock_signed_extrinsic_v4(&call_data);

			// Validate should succeed
			let result = blockchain.validate_extrinsic(&extrinsic).await;
			assert!(result.is_ok(), "Valid extrinsic should pass validation: {:?}", result);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn validate_extrinsic_rejects_garbage() {
			let ctx = create_test_context().await;
			let blockchain =
				Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

			// Submit garbage bytes
			let garbage = vec![0xde, 0xad, 0xbe, 0xef];

			let result = blockchain.validate_extrinsic(&garbage).await;
			assert!(result.is_err(), "Garbage should fail validation");
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn build_block_result_tracks_included_extrinsics() {
			use crate::{ExecutorConfig, SignatureMockMode};
			use scale::{Compact, Encode};

			let ctx = create_test_context().await;
			let config = ExecutorConfig {
				signature_mock: SignatureMockMode::AlwaysValid,
				..Default::default()
			};
			let blockchain = Blockchain::fork_with_config(&ctx.endpoint, None, None, config)
				.await
				.expect("Failed to fork");

			// Build a valid transfer extrinsic
			let head = blockchain.head().await;
			let metadata = head.metadata().await.expect("Failed to get metadata");

			let balances_pallet = metadata.pallet_by_name("Balances").expect("Balances pallet");
			let pallet_index = balances_pallet.index();
			let transfer_call = balances_pallet
				.call_variant_by_name("transfer_keep_alive")
				.expect("transfer_keep_alive");
			let call_index = transfer_call.index;

			let mut call_data = vec![pallet_index, call_index];
			call_data.push(0x00); // MultiAddress::Id
			call_data.extend(BOB);
			call_data.extend(Compact(TRANSFER_AMOUNT).encode());

			let extrinsic = build_mock_signed_extrinsic_v4(&call_data);

			let result = blockchain
				.build_block(vec![extrinsic.clone()])
				.await
				.expect("Failed to build block");

			assert_eq!(result.included.len(), 1, "Should have 1 included extrinsic");
			assert!(result.failed.is_empty(), "Should have no failed extrinsics");
			assert_eq!(result.included[0], extrinsic);
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn build_block_result_tracks_failed_extrinsics() {
			use crate::{ExecutorConfig, SignatureMockMode};
			use scale::{Compact, Encode};

			let ctx = create_test_context().await;
			let config = ExecutorConfig {
				signature_mock: SignatureMockMode::AlwaysValid,
				..Default::default()
			};
			let blockchain = Blockchain::fork_with_config(&ctx.endpoint, None, None, config)
				.await
				.expect("Failed to fork");

			// Build an extrinsic that will fail at dispatch time - transfer more than available.
			// Use a random account with no funds to trigger InsufficientBalance.
			let head = blockchain.head().await;
			let metadata = head.metadata().await.expect("Failed to get metadata");

			let balances_pallet = metadata.pallet_by_name("Balances").expect("Balances pallet");
			let pallet_index = balances_pallet.index();
			let transfer_call = balances_pallet
				.call_variant_by_name("transfer_keep_alive")
				.expect("transfer_keep_alive");
			let call_index = transfer_call.index;

			// Use a "random" account that has no funds as the sender.
			// The extrinsic is structurally valid but will fail dispatch due to lack of funds.
			let unfunded_account: [u8; 32] = [0x99; 32];
			let recipient = BOB;
			let amount: u128 = 1_000_000_000_000_000; // Large amount that unfunded account can't pay

			let mut call_data = vec![pallet_index, call_index];
			call_data.push(0x00); // MultiAddress::Id
			call_data.extend(recipient);
			call_data.extend(Compact(amount).encode());

			// Build extrinsic from unfunded account
			let extrinsic = {
				let mut inner = Vec::new();
				inner.push(0x84); // Version: signed (0x80) + v4 (0x04)
				inner.push(0x00); // MultiAddress::Id variant
				inner.extend(unfunded_account);
				inner.extend([0u8; 64]); // Dummy signature (works with AlwaysValid)
				inner.push(0x00); // CheckMortality: immortal
				inner.extend(Compact(0u64).encode()); // CheckNonce
				inner.extend(Compact(0u128).encode()); // ChargeTransactionPayment
				inner.push(0x00); // EthSetOrigin: None
				inner.extend(&call_data);
				let mut final_ext = Compact(inner.len() as u32).encode();
				final_ext.extend(inner);
				final_ext
			};

			let result = blockchain
				.build_block(vec![extrinsic.clone()])
				.await
				.expect("Build should succeed even with failed extrinsics");

			// The extrinsic should fail at dispatch (InsufficientBalance) and be in the failed list
			assert!(
				result.failed.len() == 1,
				"Failed extrinsic should be tracked. Included: {}, Failed: {}",
				result.included.len(),
				result.failed.len()
			);
			assert!(result.included.is_empty(), "Failed extrinsic should not be in included list");
			assert_eq!(result.failed[0].extrinsic, extrinsic);
		}
	}

	#[test]
	fn transaction_validity_error_reason_returns_correct_strings() {
		let stale = TransactionValidityError::Invalid(InvalidTransaction::Stale);
		assert_eq!(stale.reason(), "Nonce too low (already used)");

		let payment = TransactionValidityError::Invalid(InvalidTransaction::Payment);
		assert_eq!(payment.reason(), "Insufficient funds for fees");

		let unknown = TransactionValidityError::Unknown(UnknownTransaction::CannotLookup);
		assert_eq!(unknown.reason(), "Cannot lookup validity");
	}

	#[test]
	fn transaction_validity_error_is_unknown_distinguishes_types() {
		let invalid = TransactionValidityError::Invalid(InvalidTransaction::Stale);
		assert!(!invalid.is_unknown());

		let unknown = TransactionValidityError::Unknown(UnknownTransaction::CannotLookup);
		assert!(unknown.is_unknown());
	}

	#[test]
	fn transaction_validity_types_can_be_decoded() {
		use scale::Decode;

		// Valid transaction result (index 0)
		let valid_bytes = [
			0x00, // Ok variant
			0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // priority: 1
			0x00, // requires: empty vec
			0x00, // provides: empty vec
			0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // longevity: 64
			0x01, // propagate: true
		];
		let validity = TransactionValidity::decode(&mut valid_bytes.as_slice())
			.expect("Should decode valid transaction");
		match validity {
			TransactionValidity::Ok(valid) => {
				assert_eq!(valid.priority, 1);
				assert!(valid.requires.is_empty());
				assert!(valid.provides.is_empty());
				assert_eq!(valid.longevity, 64);
				assert!(valid.propagate);
			},
			TransactionValidity::Err(_) => panic!("Expected Ok variant"),
		}

		// Invalid transaction result (index 1) with Stale (index 3)
		let invalid_bytes = [
			0x01, // Err variant
			0x00, // Invalid variant
			0x03, // Stale
		];
		let validity = TransactionValidity::decode(&mut invalid_bytes.as_slice())
			.expect("Should decode invalid transaction");
		match validity {
			TransactionValidity::Ok(_) => panic!("Expected Err variant"),
			TransactionValidity::Err(err) => {
				assert_eq!(err.reason(), "Nonce too low (already used)");
			},
		}
	}
}
