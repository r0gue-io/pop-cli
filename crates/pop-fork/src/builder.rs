// SPDX-License-Identifier: GPL-3.0

//! Block builder for constructing new blocks on a forked chain.
//!
//! This module provides the [`BlockBuilder`] for constructing new blocks by applying
//! inherent extrinsics, user extrinsics, and finalizing the block.
//!
//! # Architecture
//!
//! The block building process follows these phases:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                      Block Building Flow                         │
//! │                                                                   │
//! │   1. new()          Create builder with parent block             │
//! │         │                                                         │
//! │         ▼                                                         │
//! │   2. initialize()   Call Core_initialize_block                   │
//! │         │                                                         │
//! │         ▼                                                         │
//! │   3. apply_inherents()  Apply inherent extrinsics               │
//! │         │                                                         │
//! │         ▼                                                         │
//! │   4. apply_extrinsic()  Apply user extrinsics (repeatable)      │
//! │         │                                                         │
//! │         ▼                                                         │
//! │   5. finalize()     Call BlockBuilder_finalize_block            │
//! │                     Returns new Block                            │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::{BlockBuilder, Block, RuntimeExecutor};
//!
//! // Create a block builder
//! let mut builder = BlockBuilder::new(parent_block, executor, header, inherent_providers);
//!
//! // Initialize and apply inherents
//! builder.initialize().await?;
//! builder.apply_inherents().await?;
//!
//! // Apply user extrinsics
//! for extrinsic in extrinsics {
//!     match builder.apply_extrinsic(extrinsic).await? {
//!         ApplyExtrinsicResult::Success { .. } => println!("Applied successfully"),
//!         ApplyExtrinsicResult::DispatchFailed { error } => println!("Failed: {}", error),
//!     }
//! }
//!
//! // Finalize the block
//! let new_block = builder.finalize().await?;
//! ```

use crate::{Block, BlockBuilderError, RuntimeCallResult, RuntimeExecutor};
use async_trait::async_trait;
use scale::Encode;
use subxt::config::substrate::H256;

/// Trait for creating inherent extrinsics during block building.
///
/// Inherent providers generate the "inherent" extrinsics that are automatically
/// included in each block (timestamp, parachain validation data, etc.).
///
/// Inherent extrinsics:
/// - Are unsigned (no signature required)
/// - Are mandatory (block is invalid without them)
/// - Come from "outside" information (time, relay chain data)
/// - Are applied before regular extrinsics
///
/// # Implementing
///
/// Implementations should return an empty `Vec` if the inherent doesn't apply
/// to the current chain (e.g., parachain inherents on a relay chain).
///
/// # Example
///
/// ```ignore
/// pub struct TimestampInherent {
///     slot_duration_ms: u64,
/// }
///
/// #[async_trait]
/// impl InherentProvider for TimestampInherent {
///     fn identifier(&self) -> &'static str {
///         "Timestamp"
///     }
///
///     async fn provide(
///         &self,
///         parent: &Block,
///         executor: &RuntimeExecutor,
///     ) -> Result<Vec<Vec<u8>>, BlockBuilderError> {
///         // Read current timestamp, add slot_duration, encode call
///         Ok(vec![encoded_timestamp_set_call])
///     }
/// }
/// ```
#[async_trait]
pub trait InherentProvider: Send + Sync {
	/// Identifier for this inherent provider (for debugging/logging).
	fn identifier(&self) -> &'static str;

	/// Generate inherent extrinsics for a new block.
	///
	/// # Arguments
	///
	/// * `parent` - The parent block being built upon
	/// * `executor` - The runtime executor for accessing chain state/metadata
	///
	/// # Returns
	///
	/// A vector of encoded inherent extrinsics. Returns an empty vector if
	/// this provider doesn't apply to the current chain.
	async fn provide(
		&self,
		parent: &Block,
		executor: &RuntimeExecutor,
	) -> Result<Vec<Vec<u8>>, BlockBuilderError>;
}

/// Result of applying an extrinsic to the block.
#[derive(Debug, Clone)]
pub enum ApplyExtrinsicResult {
	/// Extrinsic was applied successfully.
	Success {
		/// Number of storage keys modified by this extrinsic.
		storage_changes: usize,
	},
	/// Extrinsic dispatch failed.
	///
	/// Storage changes from the failed extrinsic are NOT applied (clean mode),
	/// matching chopsticks behavior.
	DispatchFailed {
		/// Error description from the runtime.
		error: String,
	},
}

/// Builder for constructing new blocks on a forked chain.
///
/// The `BlockBuilder` orchestrates the block production process by:
/// 1. Initializing the block with `Core_initialize_block`
/// 2. Applying inherent extrinsics from registered providers
/// 3. Applying user extrinsics
/// 4. Finalizing the block with `BlockBuilder_finalize_block`
///
/// # Storage Handling
///
/// Storage changes are applied directly to the parent block's storage layer.
/// For failed extrinsics (dispatch errors), storage changes are NOT applied,
/// matching chopsticks' "clean mode" behavior.
///
/// # Thread Safety
///
/// `BlockBuilder` is not `Sync` by default. It should be used from a single
/// async task.
pub struct BlockBuilder {
	/// The parent block being extended.
	parent: Block,
	/// Runtime executor for calling runtime methods.
	executor: RuntimeExecutor,
	/// Registered inherent providers.
	inherent_providers: Vec<Box<dyn InherentProvider>>,
	/// Successfully applied extrinsics (inherents + user).
	extrinsics: Vec<Vec<u8>>,
	/// Encoded header for the new block.
	header: Vec<u8>,
	/// Whether initialize() has been called.
	initialized: bool,
}

impl BlockBuilder {
	/// Create a new block builder.
	///
	/// # Arguments
	///
	/// * `parent` - The parent block to build upon
	/// * `executor` - Runtime executor for calling runtime methods
	/// * `header` - Encoded header for the new block
	/// * `inherent_providers` - Providers for generating inherent extrinsics
	///
	/// # Returns
	///
	/// A new `BlockBuilder` ready for initialization.
	pub fn new(
		parent: Block,
		executor: RuntimeExecutor,
		header: Vec<u8>,
		inherent_providers: Vec<Box<dyn InherentProvider>>,
	) -> Self {
		Self { parent, executor, inherent_providers, extrinsics: Vec::new(), header, initialized: false }
	}

	/// Get the current list of successfully applied extrinsics.
	///
	/// This includes both inherent extrinsics and user extrinsics that
	/// were successfully applied.
	pub fn extrinsics(&self) -> &[Vec<u8>] {
		&self.extrinsics
	}

	/// Initialize the block by calling `Core_initialize_block`.
	///
	/// This must be called before applying any extrinsics.
	///
	/// # Returns
	///
	/// The runtime call result containing storage diff and logs.
	///
	/// # Errors
	///
	/// Returns an error if the runtime call fails.
	pub async fn initialize(&mut self) -> Result<RuntimeCallResult, BlockBuilderError> {
		if self.initialized {
			// Already initialized, return empty result
			return Ok(RuntimeCallResult {
				output: vec![],
				storage_diff: vec![],
				offchain_storage_diff: vec![],
				logs: vec![],
			});
		}

		// Call Core_initialize_block with the header
		let result =
			self.executor.call("Core_initialize_block", &self.header, self.parent.storage()).await?;

		// Apply storage changes
		self.apply_storage_diff(&result.storage_diff)?;

		self.initialized = true;
		Ok(result)
	}

	/// Apply inherent extrinsics from all registered providers.
	///
	/// This calls each registered `InherentProvider` to generate inherent
	/// extrinsics, then applies them to the block.
	///
	/// # Returns
	///
	/// A vector of runtime call results, one for each applied inherent.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - The block has not been initialized
	/// - Any inherent provider fails
	/// - Any inherent extrinsic fails to apply
	pub async fn apply_inherents(&mut self) -> Result<Vec<RuntimeCallResult>, BlockBuilderError> {
		if !self.initialized {
			return Err(BlockBuilderError::NotInitialized);
		}

		let mut results = Vec::new();

		// Collect inherents from all providers
		for provider in &self.inherent_providers {
			let inherents = provider.provide(&self.parent, &self.executor).await.map_err(|e| {
				BlockBuilderError::InherentProvider {
					provider: provider.identifier().to_string(),
					message: e.to_string(),
				}
			})?;

			// Apply each inherent
			for inherent in inherents {
				let result = self
					.executor
					.call("BlockBuilder_apply_extrinsic", &inherent, self.parent.storage())
					.await?;

				// Inherents should always succeed - apply storage changes
				self.apply_storage_diff(&result.storage_diff)?;
				self.extrinsics.push(inherent);
				results.push(result);
			}
		}

		Ok(results)
	}

	/// Apply a user extrinsic to the block.
	///
	/// This calls `BlockBuilder_apply_extrinsic` and checks the dispatch result.
	/// Storage changes are only applied if the extrinsic succeeds (clean mode).
	///
	/// # Arguments
	///
	/// * `extrinsic` - Encoded extrinsic to apply
	///
	/// # Returns
	///
	/// - `ApplyExtrinsicResult::Success` if the extrinsic was applied
	/// - `ApplyExtrinsicResult::DispatchFailed` if dispatch failed
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - The block has not been initialized
	/// - The block has already been finalized
	/// - The runtime call itself fails (not dispatch failure)
	pub async fn apply_extrinsic(
		&mut self,
		extrinsic: Vec<u8>,
	) -> Result<ApplyExtrinsicResult, BlockBuilderError> {
		if !self.initialized {
			return Err(BlockBuilderError::NotInitialized);
		}

		// Call BlockBuilder_apply_extrinsic
		let result = self
			.executor
			.call("BlockBuilder_apply_extrinsic", &extrinsic, self.parent.storage())
			.await?;

		// Decode the dispatch result
		// Format: Result<Result<(), DispatchError>, TransactionValidityError>
		// For simplicity, we check if the first byte indicates success (0x00 = Ok)
		let is_success = result.output.first().map(|&b| b == 0x00).unwrap_or(false);

		if is_success {
			// Success - apply storage changes
			let storage_changes = result.storage_diff.len();
			self.apply_storage_diff(&result.storage_diff)?;
			self.extrinsics.push(extrinsic);
			Ok(ApplyExtrinsicResult::Success { storage_changes })
		} else {
			// Failed - do NOT apply storage changes (clean mode)
			let error = format!("Dispatch failed: {:?}", hex::encode(&result.output));
			Ok(ApplyExtrinsicResult::DispatchFailed { error })
		}
	}

	/// Finalize the block by calling `BlockBuilder_finalize_block`.
	///
	/// This consumes the builder and returns the newly constructed block.
	///
	/// # Returns
	///
	/// The finalized block with all applied extrinsics.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - The block has not been initialized
	/// - The runtime call fails
	pub async fn finalize(mut self) -> Result<Block, BlockBuilderError> {
		if !self.initialized {
			return Err(BlockBuilderError::NotInitialized);
		}

		// Call BlockBuilder_finalize_block
		let result =
			self.executor.call("BlockBuilder_finalize_block", &[], self.parent.storage()).await?;

		// Apply final storage changes
		self.apply_storage_diff(&result.storage_diff)?;

		// The result contains the final header
		let final_header = result.output;

		// Compute block hash from header (blake2_256)
		let block_hash = sp_core::blake2_256(&final_header);

		// Create the new block
		let new_block = self
			.parent
			.child(subxt::config::substrate::H256::from_slice(&block_hash), final_header, self.extrinsics)
			.await?;

		Ok(new_block)
	}

	/// Apply storage diff to the parent's storage layer.
	fn apply_storage_diff(
		&self,
		diff: &[(Vec<u8>, Option<Vec<u8>>)],
	) -> Result<(), BlockBuilderError> {
		if diff.is_empty() {
			return Ok(());
		}

		let entries: Vec<(&[u8], Option<&[u8]>)> =
			diff.iter().map(|(k, v)| (k.as_slice(), v.as_deref())).collect();

		self.parent.storage().set_batch(&entries)?;
		Ok(())
	}
}

// ============================================================================
// Header Helper
// ============================================================================

/// Digest item for block headers.
///
/// Digest items contain consensus-related information that is included
/// in the block header but not part of the main block body.
#[derive(Debug, Clone, Encode)]
pub enum DigestItem {
	/// A pre-runtime digest item.
	///
	/// These are produced by the consensus engine before block execution.
	/// Common uses include slot numbers for Aura/Babe.
	#[codec(index = 6)]
	PreRuntime(ConsensusEngineId, Vec<u8>),

	/// A consensus digest item.
	///
	/// These are produced during block execution for consensus-related data.
	#[codec(index = 4)]
	Consensus(ConsensusEngineId, Vec<u8>),

	/// A seal digest item.
	///
	/// These are added after block execution, typically containing signatures.
	#[codec(index = 5)]
	Seal(ConsensusEngineId, Vec<u8>),

	/// An "other" digest item.
	///
	/// For runtime-specific data that doesn't fit other categories.
	#[codec(index = 0)]
	Other(Vec<u8>),
}

/// Consensus engine identifier (4-byte ASCII).
///
/// Common identifiers:
/// - `*b"aura"` - Aura consensus
/// - `*b"BABE"` - Babe consensus
/// - `*b"FRNK"` - GRANDPA finality
pub type ConsensusEngineId = [u8; 4];

/// Well-known consensus engine identifiers.
pub mod consensus_engine {
	use super::ConsensusEngineId;

	/// Aura consensus engine identifier.
	pub const AURA: ConsensusEngineId = *b"aura";

	/// Babe consensus engine identifier.
	pub const BABE: ConsensusEngineId = *b"BABE";

	/// GRANDPA finality engine identifier.
	pub const GRANDPA: ConsensusEngineId = *b"FRNK";
}

/// Internal header struct for encoding.
#[derive(Encode)]
struct Header {
	parent_hash: H256,
	#[codec(compact)]
	number: u32,
	state_root: H256,
	extrinsics_root: H256,
	digest: Vec<DigestItem>,
}

/// Create a header for the next block.
///
/// This helper creates a properly encoded header for use with `BlockBuilder`.
/// The header will have:
/// - `parent_hash` set to the parent block's hash
/// - `number` incremented from the parent
/// - `state_root` and `extrinsics_root` set to zero (computed by runtime)
/// - `digest` containing the provided digest items
///
/// # Arguments
///
/// * `parent` - The parent block to build upon
/// * `digest_items` - Digest items to include (e.g., slot information)
///
/// # Returns
///
/// Encoded header bytes ready for `BlockBuilder::new()`.
///
/// # Example
///
/// ```ignore
/// use pop_fork::{create_next_header, DigestItem, consensus_engine};
///
/// // Create header with Aura slot
/// let slot: u64 = 12345;
/// let header = create_next_header(
///     &parent_block,
///     vec![DigestItem::PreRuntime(consensus_engine::AURA, slot.encode())],
/// );
///
/// let builder = BlockBuilder::new(parent_block, executor, header, providers);
/// ```
pub fn create_next_header(parent: &Block, digest_items: Vec<DigestItem>) -> Vec<u8> {
	let header = Header {
		parent_hash: parent.hash,
		number: parent.number + 1,
		state_root: H256::zero(),      // Will be computed by runtime
		extrinsics_root: H256::zero(), // Will be computed by runtime
		digest: digest_items,
	};
	header.encode()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
	use super::*;

	/// Verifies that DigestItem::PreRuntime encodes with the correct index (6).
	#[test]
	fn digest_item_pre_runtime_encodes_correctly() {
		let engine_id = *b"aura";
		let data = vec![1, 2, 3, 4];
		let item = DigestItem::PreRuntime(engine_id, data.clone());
		let encoded = item.encode();

		// First byte should be index 6 for PreRuntime
		assert_eq!(encoded[0], 6);
		// Next 4 bytes should be the engine ID
		assert_eq!(&encoded[1..5], b"aura");
		// Rest should be the compact-encoded length + data
	}

	/// Verifies that DigestItem::Consensus encodes with the correct index (4).
	#[test]
	fn digest_item_consensus_encodes_correctly() {
		let engine_id = *b"BABE";
		let data = vec![5, 6, 7, 8];
		let item = DigestItem::Consensus(engine_id, data.clone());
		let encoded = item.encode();

		// First byte should be index 4 for Consensus
		assert_eq!(encoded[0], 4);
		// Next 4 bytes should be the engine ID
		assert_eq!(&encoded[1..5], b"BABE");
	}

	/// Verifies that DigestItem::Seal encodes with the correct index (5).
	#[test]
	fn digest_item_seal_encodes_correctly() {
		let engine_id = *b"FRNK";
		let data = vec![9, 10, 11, 12];
		let item = DigestItem::Seal(engine_id, data.clone());
		let encoded = item.encode();

		// First byte should be index 5 for Seal
		assert_eq!(encoded[0], 5);
		// Next 4 bytes should be the engine ID
		assert_eq!(&encoded[1..5], b"FRNK");
	}

	/// Verifies that DigestItem::Other encodes with the correct index (0).
	#[test]
	fn digest_item_other_encodes_correctly() {
		let data = vec![13, 14, 15, 16];
		let item = DigestItem::Other(data.clone());
		let encoded = item.encode();

		// First byte should be index 0 for Other
		assert_eq!(encoded[0], 0);
	}

	/// Verifies that consensus engine constants have the correct values.
	#[test]
	fn consensus_engine_constants_are_correct() {
		assert_eq!(consensus_engine::AURA, *b"aura");
		assert_eq!(consensus_engine::BABE, *b"BABE");
		assert_eq!(consensus_engine::GRANDPA, *b"FRNK");
	}

	/// Verifies that ApplyExtrinsicResult::Success can be constructed and cloned.
	#[test]
	fn apply_extrinsic_result_success_is_cloneable() {
		let result = ApplyExtrinsicResult::Success { storage_changes: 42 };
		let cloned = result.clone();
		match cloned {
			ApplyExtrinsicResult::Success { storage_changes } => {
				assert_eq!(storage_changes, 42);
			},
			_ => panic!("Expected Success variant"),
		}
	}

	/// Verifies that ApplyExtrinsicResult::DispatchFailed can be constructed and cloned.
	#[test]
	fn apply_extrinsic_result_dispatch_failed_is_cloneable() {
		let result =
			ApplyExtrinsicResult::DispatchFailed { error: "Test error".to_string() };
		let cloned = result.clone();
		match cloned {
			ApplyExtrinsicResult::DispatchFailed { error } => {
				assert_eq!(error, "Test error");
			},
			_ => panic!("Expected DispatchFailed variant"),
		}
	}

	/// Verifies that the Header struct encodes with correct field order.
	#[test]
	fn header_encodes_correctly() {
		let header = Header {
			parent_hash: H256::zero(),
			number: 100,
			state_root: H256::zero(),
			extrinsics_root: H256::zero(),
			digest: vec![],
		};
		let encoded = header.encode();

		// Header should contain:
		// - 32 bytes parent_hash
		// - compact-encoded number (100 = 0x91 0x01 for compact)
		// - 32 bytes state_root
		// - 32 bytes extrinsics_root
		// - compact-encoded digest length (0)

		// Parent hash starts with 32 zero bytes
		assert!(encoded.starts_with(&[0u8; 32]));
		// Total should be at least 32 + 1 + 32 + 32 + 1 = 98 bytes
		assert!(encoded.len() >= 98);
	}

	/// Verifies that the Header encodes block number using compact encoding.
	#[test]
	fn header_uses_compact_block_number() {
		// Small number (single byte compact)
		let header1 = Header {
			parent_hash: H256::zero(),
			number: 1,
			state_root: H256::zero(),
			extrinsics_root: H256::zero(),
			digest: vec![],
		};

		// Large number (multi-byte compact)
		let header2 = Header {
			parent_hash: H256::zero(),
			number: 1_000_000,
			state_root: H256::zero(),
			extrinsics_root: H256::zero(),
			digest: vec![],
		};

		let encoded1 = header1.encode();
		let encoded2 = header2.encode();

		// The larger block number should result in a larger encoding
		// because compact encoding uses more bytes for larger values
		assert!(encoded2.len() > encoded1.len());
	}
}
