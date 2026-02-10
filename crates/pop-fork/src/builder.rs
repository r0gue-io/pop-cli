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
//! │                      Block Building Flow                        │
//! │                                                                 │
//! │   1. new()          Create builder with parent block            │
//! │         │                                                       │
//! │         ▼                                                       │
//! │   2. initialize()   Call Core_initialize_block                  │
//! │         │                                                       │
//! │         ▼                                                       │
//! │   3. apply_inherents()  Apply inherent extrinsics               │
//! │         │                                                       │
//! │         ▼                                                       │
//! │   4. apply_extrinsic()  Apply user extrinsics (repeatable)      │
//! │         │                                                       │
//! │         ▼                                                       │
//! │   5. finalize()     Call BlockBuilder_finalize_block            │
//! │                     Returns new Block                           │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::{BlockBuilder, Block, RuntimeExecutor};
//!
//! // Create a block builder
//! let mut builder = BlockBuilder::new(parent_block, executor, header, inherent_providers, None, false);
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
//! let (new_block, _prototype) = builder.finalize().await?;
//! ```

use crate::{
	Block, BlockBuilderError, RuntimeCallResult, RuntimeExecutor,
	inherent::{
		InherentProvider,
		relay::{PARA_INHERENT_PALLET, para_inherent_included_key},
	},
	strings::{
		builder::runtime_api,
		inherent::timestamp::slot_duration::{
			PARACHAIN_FALLBACK_MS as DEFAULT_PARA_SLOT_DURATION_MS,
			RELAY_CHAIN_FALLBACK_MS as DEFAULT_RELAY_SLOT_DURATION_MS,
		},
	},
};
use log::{error, info};
use scale::{Decode, Encode};
use smoldot::executor::host::HostVmPrototype;
use sp_core::blake2_256;
use subxt::{Metadata, config::substrate::H256, metadata::types::StorageEntryType};

/// Phase of the block building process.
///
/// Tracks the current state of the builder to enforce correct ordering:
/// `Created` → `Initialized` → `InherentsApplied` → (extrinsics) → finalize
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuilderPhase {
	/// Builder created, `initialize()` not yet called.
	#[default]
	Created,
	/// Block initialized via `Core_initialize_block`, ready for inherents.
	Initialized,
	/// Inherents applied, ready for user extrinsics and finalization.
	InherentsApplied,
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
	/// Storage changes from the failed extrinsic are NOT applied.
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
/// For failed extrinsics (dispatch errors), storage changes are NOT applied.
///
/// # Thread Safety
///
/// `BlockBuilder` is not `Sync` by default. It should be used from a single
/// async task.
///
/// # Example
///
/// ```ignore
/// use pop_fork::{Block, BlockBuilder, RuntimeExecutor, create_next_header};
///
/// // Create header for the new block
/// let header = create_next_header(&parent_block, vec![]);
///
/// // Create builder with inherent providers
/// let mut builder = BlockBuilder::new(parent_block, executor, header, inherent_providers, None, false);
///
/// // Build the block
/// builder.initialize().await?;
/// builder.apply_inherents().await?;
///
/// // Apply user extrinsics
/// for extrinsic in user_extrinsics {
///     match builder.apply_extrinsic(extrinsic).await? {
///         ApplyExtrinsicResult::Success { storage_changes } => {
///             println!("Applied with {} storage changes", storage_changes);
///         }
///         ApplyExtrinsicResult::DispatchFailed { error } => {
///             println!("Dispatch failed: {}", error);
///         }
///     }
/// }
///
/// // Finalize and get the new block
/// let (new_block, _prototype) = builder.finalize().await?;
/// ```
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
	/// Current phase of block building.
	phase: BuilderPhase,
	/// Reusable VM prototype to avoid re-parsing WASM on each runtime call.
	prototype: Option<HostVmPrototype>,
	/// Whether to skip the storage prefetch during initialization.
	///
	/// After the first block build, all `StorageValue` keys and pallet prefix
	/// pages are already cached in the `RemoteStorageLayer`. Repeating the
	/// prefetch on every block wastes time iterating metadata and issuing
	/// no-op cache lookups.
	skip_prefetch: bool,
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
	/// * `prototype` - Optional warm VM prototype to reuse from a previous block build. Passing a
	///   compiled prototype avoids re-parsing and re-compiling the WASM runtime, which is
	///   significant for large runtimes (~2.5 MB for Asset Hub).
	/// * `skip_prefetch` - When `true`, skip the storage prefetch during initialization. Set this
	///   after the first block build since all keys are already cached.
	///
	/// # Returns
	///
	/// A new `BlockBuilder` ready for initialization.
	pub fn new(
		parent: Block,
		executor: RuntimeExecutor,
		header: Vec<u8>,
		inherent_providers: Vec<Box<dyn InherentProvider>>,
		prototype: Option<HostVmPrototype>,
		skip_prefetch: bool,
	) -> Self {
		Self {
			parent,
			executor,
			inherent_providers,
			extrinsics: Vec::new(),
			header,
			phase: BuilderPhase::Created,
			prototype,
			skip_prefetch,
		}
	}

	/// Get the current list of successfully applied extrinsics.
	///
	/// This includes both inherent extrinsics and user extrinsics that
	/// were successfully applied.
	pub fn extrinsics(&self) -> &[Vec<u8>] {
		&self.extrinsics
	}

	/// Get the current phase of block building.
	pub fn phase(&self) -> BuilderPhase {
		self.phase
	}

	/// Get the storage layer for the current fork.
	///
	/// The storage layer is shared across all blocks in the fork and tracks
	/// all modifications. This provides access to the current working state.
	fn storage(&self) -> &crate::LocalStorageLayer {
		self.parent.storage()
	}

	/// Reset storage access counters. Call before a phase to measure it in isolation.
	fn reset_storage_stats(&self) {
		self.storage().remote().reset_stats();
	}

	/// Log the current storage access counters for a phase.
	fn log_storage_stats(&self, phase: &str) {
		let stats = self.storage().remote().stats();
		info!("[BlockBuilder] {phase} storage: {stats}");
	}

	/// Prefetch storage commonly accessed during block building.
	///
	/// Uses two strategies to pre-populate the cache before runtime execution:
	///
	/// 1. **StorageValue batch**: Fetches every `Plain` (single-key) storage item from metadata in
	///    one RPC call. These are cheap and almost always read during block execution.
	///
	/// 2. **Single-page pallet scans**: For each pallet, fetches the first page (up to 200 keys) of
	///    its 16-byte prefix in parallel. This covers small pallets entirely and pre-warms the
	///    most-accessed slice of larger ones. Individual storage maps that aren't covered get
	///    picked up by the speculative prefetch in `RemoteStorageLayer::get()` during execution.
	///
	/// Together these eliminate the vast majority of individual RPC round-trips
	/// that would otherwise block WASM execution.
	async fn prefetch_block_building_storage(&self) -> Result<(), BlockBuilderError> {
		let remote = self.storage().remote();
		let block_hash = self.storage().fork_block_hash();
		let metadata = self.parent.metadata().await?;

		// --- 1. Batch-fetch all StorageValue keys ---
		let mut value_keys: Vec<Vec<u8>> = Vec::new();
		let mut pallet_prefixes: Vec<Vec<u8>> = Vec::new();

		for pallet in metadata.pallets() {
			let pallet_hash = sp_core::twox_128(pallet.name().as_bytes());

			if let Some(storage) = pallet.storage() {
				for entry in storage.entries() {
					if matches!(entry.entry_type(), StorageEntryType::Plain(_)) {
						let entry_hash = sp_core::twox_128(entry.name().as_bytes());
						value_keys.push([pallet_hash.as_slice(), entry_hash.as_slice()].concat());
					}
				}
				pallet_prefixes.push(pallet_hash.to_vec());
			}
		}

		if !value_keys.is_empty() {
			let key_refs: Vec<&[u8]> = value_keys.iter().map(|k| k.as_slice()).collect();
			if let Err(e) = remote.get_batch(block_hash, &key_refs).await {
				log::debug!("[BlockBuilder] StorageValue batch fetch failed (non-fatal): {e}");
			}
		}

		// --- 2. Single-page pallet scans (in parallel) ---
		// Individual storage maps not covered here get picked up by the
		// speculative prefetch at the 32-byte level during execution.
		// Scan failures are non-fatal: the speculative prefetch and individual
		// fetches during execution will pick up any keys we missed here.
		let page_size = crate::strings::builder::PREFETCH_PAGE_SIZE;
		let scan_futures: Vec<_> = pallet_prefixes
			.iter()
			.map(|prefix| remote.prefetch_prefix_single_page(block_hash, prefix, page_size))
			.collect();
		let scan_results = futures::future::join_all(scan_futures).await;
		let mut scan_keys = 0usize;
		let mut scan_errors = 0usize;
		for result in scan_results {
			match result {
				Ok(count) => scan_keys += count,
				Err(e) => {
					scan_errors += 1;
					log::debug!("[BlockBuilder] Pallet scan failed (non-fatal): {e}");
				},
			}
		}

		if scan_errors > 0 {
			info!(
				"[BlockBuilder] Prefetched {} StorageValue + {} map keys ({} pallets, {} scans failed)",
				value_keys.len(),
				scan_keys,
				pallet_prefixes.len(),
				scan_errors,
			);
		} else {
			info!(
				"[BlockBuilder] Prefetched {} StorageValue + {} map keys ({} pallets)",
				value_keys.len(),
				scan_keys,
				pallet_prefixes.len(),
			);
		}
		Ok(())
	}

	/// Initialize the block by calling `Core_initialize_block`.
	///
	/// This must be called before applying any inherents or extrinsics.
	/// Can only be called once (in `Created` phase).
	///
	/// # Returns
	///
	/// The runtime call result containing storage diff and logs.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - The block has already been initialized
	/// - The runtime call fails
	pub async fn initialize(&mut self) -> Result<RuntimeCallResult, BlockBuilderError> {
		if self.phase != BuilderPhase::Created {
			// Already past Created phase
			return Err(BlockBuilderError::AlreadyInitialized);
		}

		// Prefetch storage keys commonly accessed during block building.
		// Skipped for subsequent blocks since keys are already cached.
		if !self.skip_prefetch {
			info!("[BlockBuilder] Prefetching block building storage...");
			self.prefetch_block_building_storage().await?;
		}

		// Call Core_initialize_block with the header
		info!("[BlockBuilder] Calling Core_initialize_block...");
		self.reset_storage_stats();
		let (result, proto) = self
			.executor
			.call_with_prototype(
				self.prototype.take(),
				runtime_api::CORE_INITIALIZE_BLOCK,
				&self.header,
				self.storage(),
			)
			.await;
		self.prototype = proto;
		let result = result.map_err(|e| {
			error!("[BlockBuilder] Core_initialize_block FAILED: {e}");
			e
		})?;
		self.log_storage_stats("Core_initialize_block");
		info!("[BlockBuilder] Core_initialize_block OK");
		info!(
			"[BlockBuilder] Building block on top of #{} (0x{}...)",
			self.parent.number,
			hex::encode(&self.parent.hash.0[..4])
		);

		// Apply storage changes
		self.apply_storage_diff(&result.storage_diff)?;

		self.phase = BuilderPhase::Initialized;
		Ok(result)
	}

	/// Apply inherent extrinsics from all registered providers.
	///
	/// This calls each registered `InherentProvider` to generate inherent
	/// extrinsics, then applies them to the block. Can only be called once,
	/// after `initialize()` and before any `apply_extrinsic()` calls.
	///
	/// # Returns
	///
	/// A vector of runtime call results, one for each applied inherent.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - The block has not been initialized
	/// - Inherents have already been applied
	/// - Any inherent provider fails
	/// - Any inherent extrinsic fails to apply
	pub async fn apply_inherents(&mut self) -> Result<Vec<RuntimeCallResult>, BlockBuilderError> {
		match self.phase {
			BuilderPhase::Created => return Err(BlockBuilderError::NotInitialized),
			BuilderPhase::InherentsApplied =>
				return Err(BlockBuilderError::InherentsAlreadyApplied),
			BuilderPhase::Initialized => {}, // Expected phase
		}

		self.reset_storage_stats();
		let mut results = Vec::new();

		// Collect inherents from all providers first to avoid borrow conflicts
		let mut all_inherents: Vec<(String, Vec<Vec<u8>>)> = Vec::new();
		for provider in &self.inherent_providers {
			let id = provider.identifier().to_string();
			info!("[BlockBuilder] Getting inherents from provider: {}", id);
			let inherents = provider.provide(&self.parent, &self.executor).await.map_err(|e| {
				error!("[BlockBuilder] Provider {} FAILED: {e}", id);
				BlockBuilderError::InherentProvider { provider: id.clone(), message: e.to_string() }
			})?;
			all_inherents.push((id, inherents));
		}

		// Apply collected inherents
		for (provider_id, inherents) in &all_inherents {
			for (i, inherent) in inherents.iter().enumerate() {
				let result = self.call_apply_extrinsic(inherent).await.map_err(|e| {
					error!("[BlockBuilder] Inherent {i} from {} FAILED: {e}", provider_id);
					e
				})?;
				// Check dispatch result - format: Result<Result<(), DispatchError>,
				// TransactionValidityError> First byte: 0x00 = Ok (applied), 0x01 = Err
				// (transaction invalid) If Ok, second byte: 0x00 = dispatch success, 0x01 =
				// dispatch error
				let dispatch_ok = match (result.output.first(), result.output.get(1)) {
					(Some(0x00), Some(0x00)) => {
						info!(
							"[BlockBuilder] Inherent {i} from {} OK (dispatch success)",
							provider_id
						);
						true
					},
					(Some(0x00), Some(0x01)) => {
						error!(
							"[BlockBuilder] Inherent {i} from {} DISPATCH FAILED: {:?}",
							provider_id,
							hex::encode(&result.output)
						);
						false
					},
					(Some(0x01), _) => {
						error!(
							"[BlockBuilder] Inherent {i} from {} INVALID: {:?}",
							provider_id,
							hex::encode(&result.output)
						);
						false
					},
					_ => false,
				};

				// For inherents, dispatch failures are fatal
				if !dispatch_ok {
					return Err(BlockBuilderError::InherentProvider {
						provider: provider_id.clone(),
						message: format!(
							"Inherent dispatch failed: {}",
							hex::encode(&result.output)
						),
					});
				}

				// Apply storage changes
				self.apply_storage_diff(&result.storage_diff)?;
				self.extrinsics.push(inherent.clone());
				results.push(result);
			}
		}

		// Mock relay chain storage if needed
		self.mock_relay_chain_inherent().await?;

		self.log_storage_stats("apply_inherents");
		self.phase = BuilderPhase::InherentsApplied;
		Ok(results)
	}

	/// Mock the `ParaInherent::Included` storage for relay chain runtimes.
	///
	/// Relay chains require `ParaInherent::Included` to be set every block,
	/// otherwise `on_finalize` panics. Instead of constructing a valid
	/// `paras_inherent.enter` extrinsic, we directly set the storage.
	///
	/// This is a no-op for non-relay chains (chains without `ParaInherent` pallet).
	async fn mock_relay_chain_inherent(&self) -> Result<(), BlockBuilderError> {
		let metadata = self.parent.metadata().await?;

		// Check if this is a relay chain (has ParaInherent pallet)
		if metadata.pallet_by_name(PARA_INHERENT_PALLET).is_none() {
			return Ok(());
		}

		// Set ParaInherent::Included to Some(())
		// The value is () which encodes to empty bytes, but FRAME stores Some(()) as existing key
		let key = para_inherent_included_key();
		self.storage().set(&key, Some(&().encode()))?;

		Ok(())
	}

	/// Apply a user extrinsic to the block.
	///
	/// This calls `BlockBuilder_apply_extrinsic` and checks the dispatch result.
	/// Storage changes are only applied if the extrinsic succeeds.
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
	/// - Inherents have not been applied yet
	/// - The runtime call itself fails (not dispatch failure)
	pub async fn apply_extrinsic(
		&mut self,
		extrinsic: Vec<u8>,
	) -> Result<ApplyExtrinsicResult, BlockBuilderError> {
		match self.phase {
			BuilderPhase::Created => return Err(BlockBuilderError::NotInitialized),
			BuilderPhase::Initialized => return Err(BlockBuilderError::InherentsNotApplied),
			BuilderPhase::InherentsApplied => {}, // Expected phase
		}

		self.reset_storage_stats();
		let result = self.call_apply_extrinsic(&extrinsic).await?;

		// Decode the dispatch result
		// Format: Result<Result<(), DispatchError>, TransactionValidityError>
		// For simplicity, we check if the first byte indicates success (0x00 = Ok)
		let is_success = result.output.first().map(|&b| b == 0x00).unwrap_or(false);

		if is_success {
			// Success - apply storage changes
			let storage_changes = result.storage_diff.len();
			self.apply_storage_diff(&result.storage_diff)?;
			let ext_hash = blake2_256(&extrinsic);
			self.log_storage_stats("apply_extrinsic");
			info!(
				"[BlockBuilder] Extrinsic 0x{}...{} included in block",
				hex::encode(&ext_hash[..4]),
				hex::encode(&ext_hash[28..])
			);
			self.extrinsics.push(extrinsic);
			Ok(ApplyExtrinsicResult::Success { storage_changes })
		} else {
			// Failed - do NOT apply storage changes.
			let error = format!("Dispatch failed: {:?}", hex::encode(&result.output));
			Ok(ApplyExtrinsicResult::DispatchFailed { error })
		}
	}

	/// Call the `BlockBuilder_apply_extrinsic` runtime API.
	///
	/// This is a helper function that executes the runtime call without
	/// interpreting the result or applying storage changes.
	async fn call_apply_extrinsic(
		&mut self,
		extrinsic: &[u8],
	) -> Result<RuntimeCallResult, BlockBuilderError> {
		let (result, proto) = self
			.executor
			.call_with_prototype(
				self.prototype.take(),
				runtime_api::BLOCK_BUILDER_APPLY_EXTRINSIC,
				extrinsic,
				self.storage(),
			)
			.await;
		self.prototype = proto;
		result.map_err(Into::into)
	}

	/// Finalize the block by calling `BlockBuilder_finalize_block`.
	///
	/// This consumes the builder and returns the newly constructed block along
	/// with the warm VM prototype. The prototype can be passed to the next
	/// [`BlockBuilder::new`] call to avoid re-compiling the WASM runtime.
	///
	/// # Returns
	///
	/// A tuple of `(block, prototype)` where `prototype` is the warm VM prototype
	/// that can be reused for the next block build.
	///
	/// # Errors
	///
	/// Returns an error if:
	/// - The block has not been initialized
	/// - Inherents have not been applied
	/// - The runtime call fails
	pub async fn finalize(mut self) -> Result<(Block, Option<HostVmPrototype>), BlockBuilderError> {
		match self.phase {
			BuilderPhase::Created => return Err(BlockBuilderError::NotInitialized),
			BuilderPhase::Initialized => return Err(BlockBuilderError::InherentsNotApplied),
			BuilderPhase::InherentsApplied => {}, // Expected phase
		}

		// Call BlockBuilder_finalize_block
		info!("[BlockBuilder] Calling BlockBuilder_finalize_block...");
		self.reset_storage_stats();
		let (result, proto) = self
			.executor
			.call_with_prototype(
				self.prototype.take(),
				runtime_api::BLOCK_BUILDER_FINALIZE_BLOCK,
				&[],
				self.storage(),
			)
			.await;
		self.prototype = proto;
		let result = result.map_err(|e| {
			error!("[BlockBuilder] BlockBuilder_finalize_block FAILED: {e}");
			e
		})?;
		self.log_storage_stats("finalize_block");
		info!("[BlockBuilder] BlockBuilder_finalize_block OK");

		// Apply final storage changes
		self.apply_storage_diff(&result.storage_diff)?;

		// Check if runtime code changed in the parent block (runtime upgrade occurred).
		// If code changed in parent block X, block X+1 (current) uses the new runtime,
		// so we need to register the new metadata at current block.
		if self.storage().has_code_changed_at(self.parent.number)? {
			self.register_new_metadata().await?;
		}

		// The result contains the final header
		let final_header = result.output;

		// Compute block hash from header (blake2_256)
		let block_hash = sp_core::blake2_256(&final_header);

		// Create the new block
		let new_block = self
			.parent
			.child(
				subxt::config::substrate::H256::from_slice(&block_hash),
				final_header,
				self.extrinsics,
			)
			.await?;

		// Extract the warm prototype for reuse in the next block build.
		// If a runtime upgrade occurred in the current block, the prototype
		// is stale and should be discarded by the caller.
		let prototype = self.prototype.take();

		Ok((new_block, prototype))
	}

	/// Check if a runtime upgrade occurred during this block's execution.
	///
	/// Returns `true` if `:code` was modified in the current block, meaning
	/// the next block will need a fresh executor and prototype compiled from
	/// the new runtime code.
	pub fn runtime_upgraded(&self) -> bool {
		let current_block = self.storage().get_current_block_number();
		self.storage().has_code_changed_at(current_block).unwrap_or(false)
	}

	/// Register new metadata after a runtime upgrade.
	///
	/// This is called when the `:code` storage key was modified in the parent block,
	/// indicating a runtime upgrade occurred. Since the BlockBuilder's executor is
	/// already running the new runtime (it was initialized with code from current state),
	/// we simply call `Metadata_metadata` using the existing executor and register
	/// the metadata for the current block (the first block using the new runtime).
	async fn register_new_metadata(&mut self) -> Result<(), BlockBuilderError> {
		let current_block_number = self.storage().get_current_block_number();

		// The executor is already running the new runtime (initialized from current state
		// which includes the parent block's code change), so we can use it directly
		let (result, proto) = self
			.executor
			.call_with_prototype(
				self.prototype.take(),
				runtime_api::METADATA_METADATA,
				&[],
				self.storage(),
			)
			.await;
		self.prototype = proto;
		let metadata_result = result?;

		// Decode the metadata (output is OpaqueMetadata which is just Vec<u8>)
		// The actual metadata is SCALE-encoded inside
		let metadata_bytes: Vec<u8> = Decode::decode(&mut metadata_result.output.as_slice())
			.map_err(|e| {
				BlockBuilderError::Codec(format!("Failed to decode metadata wrapper: {}", e))
			})?;

		let new_metadata = Metadata::decode(&mut metadata_bytes.as_slice())
			.map_err(|e| BlockBuilderError::Codec(format!("Failed to decode metadata: {}", e)))?;

		// Register the new metadata version for the current block
		// (the first block using the new runtime)
		self.storage().register_metadata_version(current_block_number, new_metadata)?;

		Ok(())
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

		self.storage().set_batch(&entries)?;
		Ok(())
	}
}

/// Digest item for block headers.
///
/// Digest items contain consensus-related information that is included
/// in the block header but not part of the main block body.
#[derive(Debug, Clone, Encode, Decode)]
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
/// let builder = BlockBuilder::new(parent_block, executor, header, providers, None, false);
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

/// Create a header for the next block with automatic slot digest injection.
///
/// This function automatically detects the consensus type (Aura/Babe) and
/// injects the appropriate slot digest into the header. The slot is calculated
/// based on the parent block's timestamp and slot duration.
///
/// # Arguments
///
/// * `parent` - The parent block to build upon
/// * `executor` - Runtime executor for calling runtime APIs
/// * `additional_digest_items` - Additional digest items to include (e.g., seal)
///
/// # Returns
///
/// Encoded header bytes ready for `BlockBuilder::new()`.
///
/// # Slot Calculation
///
/// The slot is calculated as:
/// ```text
/// next_timestamp = current_timestamp + slot_duration
/// next_slot = next_timestamp / slot_duration
/// ```
///
/// # Consensus Detection
///
/// The function detects the consensus type by checking runtime metadata:
/// - If `Aura` pallet exists → inject `PreRuntime(*b"aura", slot)`
/// - If `Babe` pallet exists → inject `PreRuntime(*b"BABE", slot)`
/// - Otherwise → no slot injection
///
/// # Example
///
/// ```ignore
/// use pop_fork::{create_next_header_with_slot, Block, RuntimeExecutor};
///
/// // Create header with automatic slot detection and injection
/// let header = create_next_header_with_slot(&parent, &executor, vec![]).await?;
/// let builder = BlockBuilder::new(parent_block, executor, header, providers, None, false);
/// ```
pub async fn create_next_header_with_slot(
	parent: &Block,
	executor: &RuntimeExecutor,
	additional_digest_items: Vec<DigestItem>,
	cached_slot_duration: Option<u64>,
) -> Result<Vec<u8>, BlockBuilderError> {
	use crate::inherent::{
		TimestampInherent,
		slot::{
			ConsensusType, calculate_next_slot, detect_consensus_type, encode_aura_slot,
			encode_babe_predigest,
		},
	};

	let metadata = parent.metadata().await?;
	let storage = parent.storage();

	// Detect consensus type from metadata
	let consensus_type = detect_consensus_type(&metadata);

	// Build digest items
	let mut digest_items = Vec::new();

	// Check if caller already provided a PreRuntime digest for this consensus
	let has_preruntime = additional_digest_items.iter().any(|item| match item {
		DigestItem::PreRuntime(engine, _) =>
			(consensus_type == ConsensusType::Aura && *engine == consensus_engine::AURA) ||
				(consensus_type == ConsensusType::Babe && *engine == consensus_engine::BABE),
		_ => false,
	});

	// Inject slot digest if needed
	if !has_preruntime && consensus_type != ConsensusType::Unknown {
		// Get slot duration - use parachain default for Aura (most common),
		// relay default for Babe
		let default_duration = match consensus_type {
			ConsensusType::Aura => DEFAULT_PARA_SLOT_DURATION_MS,
			ConsensusType::Babe => DEFAULT_RELAY_SLOT_DURATION_MS,
			ConsensusType::Unknown => DEFAULT_RELAY_SLOT_DURATION_MS,
		};

		let slot_duration = match cached_slot_duration {
			Some(d) => d,
			None =>
				TimestampInherent::get_slot_duration_from_runtime(
					executor,
					storage,
					&metadata,
					default_duration,
				)
				.await,
		};

		// Read current timestamp from storage
		let timestamp_key = TimestampInherent::timestamp_now_key();
		let current_timestamp = match storage.get(parent.number, &timestamp_key).await? {
			Some(value) if value.value.is_some() => {
				let bytes = value.value.as_ref().expect("checked above");
				u64::decode(&mut bytes.as_slice()).unwrap_or(0)
			},
			_ => {
				// Genesis or early block: use system time as fallback
				std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.map(|d| d.as_millis() as u64)
					.unwrap_or(0)
			},
		};

		// Calculate next slot
		let next_slot = calculate_next_slot(current_timestamp, slot_duration);

		// Create the appropriate PreRuntime digest
		// Aura: just the slot encoded as u64
		// Babe: a PreDigest::SecondaryPlain struct with authority_index and slot
		let (engine, slot_bytes) = match consensus_type {
			ConsensusType::Aura => (consensus_engine::AURA, encode_aura_slot(next_slot)),
			ConsensusType::Babe => {
				// Use authority_index 0 for forked execution (we're not a real validator)
				(consensus_engine::BABE, encode_babe_predigest(next_slot, 0))
			},
			ConsensusType::Unknown => unreachable!("checked above"),
		};

		digest_items.push(DigestItem::PreRuntime(engine, slot_bytes));
	}

	// Add caller-provided digest items
	digest_items.extend(additional_digest_items);

	// Create and encode header using the existing function
	Ok(create_next_header(parent, digest_items))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Verifies that consensus engine constants have the correct values.
	#[test]
	fn consensus_engine_constants_are_correct() {
		assert_eq!(consensus_engine::AURA, *b"aura");
		assert_eq!(consensus_engine::BABE, *b"BABE");
		assert_eq!(consensus_engine::GRANDPA, *b"FRNK");
	}

	/// Integration tests that execute BlockBuilder against a local test node.
	///
	/// These tests verify the full block building lifecycle including
	/// initialization, inherent application, and finalization.
	mod sequential {
		use super::*;
		use crate::{Block, ExecutorConfig, RuntimeExecutor, testing::TestContext};

		/// Test context holding all components needed for block building.
		struct BlockBuilderTestContext {
			#[allow(dead_code)]
			base: TestContext,
			block: Block,
			executor: RuntimeExecutor,
		}

		/// Creates a fully initialized block builder test context with default executor config.
		async fn create_test_context() -> BlockBuilderTestContext {
			create_test_context_with_config(None).await
		}

		/// Creates a test context with optional custom executor configuration.
		async fn create_test_context_with_config(
			config: Option<ExecutorConfig>,
		) -> BlockBuilderTestContext {
			let base = TestContext::for_storage().await;

			let block_hash = base.block_hash();

			// Fetch runtime code for the executor
			let runtime_code =
				base.rpc().runtime_code(block_hash).await.expect("Failed to fetch runtime code");

			// Create fork point block
			let block = Block::fork_point(&base.endpoint, base.cache().clone(), block_hash.into())
				.await
				.expect("Failed to create fork point");

			// Create executor with optional custom config
			let executor = match config {
				Some(cfg) => RuntimeExecutor::with_config(runtime_code, None, cfg),
				None => RuntimeExecutor::new(runtime_code, None),
			}
			.expect("Failed to create executor");

			BlockBuilderTestContext { base, block, executor }
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn new_creates_builder_with_empty_extrinsics() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);

			assert!(builder.extrinsics().is_empty());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn initialize_succeeds_and_modifies_storage() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
			let result = builder.initialize().await.expect("initialize failed");

			// Core_initialize_block should modify storage
			assert!(!result.storage_diff.is_empty());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn initialize_twice_fails() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);

			// First initialize
			builder.initialize().await.expect("first initialize failed");

			// Second initialize should fail
			let result = builder.initialize().await;
			assert!(matches!(result, Err(BlockBuilderError::AlreadyInitialized)));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn apply_inherents_without_providers_returns_empty() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
			builder.initialize().await.expect("initialize failed");

			let results = builder.apply_inherents().await.expect("apply_inherents failed");

			assert!(results.is_empty());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn apply_inherents_before_initialize_fails() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);

			let result = builder.apply_inherents().await;

			assert!(matches!(result, Err(BlockBuilderError::NotInitialized)));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn apply_extrinsic_before_initialize_fails() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);

			let result = builder.apply_extrinsic(vec![0x00]).await;

			assert!(matches!(result, Err(BlockBuilderError::NotInitialized)));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn finalize_before_initialize_fails() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);

			let result = builder.finalize().await;

			assert!(matches!(result, Err(BlockBuilderError::NotInitialized)));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn apply_inherents_twice_fails() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
			builder.initialize().await.expect("initialize failed");

			// First apply_inherents
			builder.apply_inherents().await.expect("first apply_inherents failed");

			// Second apply_inherents should fail
			let result = builder.apply_inherents().await;
			assert!(matches!(result, Err(BlockBuilderError::InherentsAlreadyApplied)));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn apply_extrinsic_before_inherents_fails() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
			builder.initialize().await.expect("initialize failed");

			// Try to apply extrinsic without applying inherents first
			let result = builder.apply_extrinsic(vec![0x00]).await;
			assert!(matches!(result, Err(BlockBuilderError::InherentsNotApplied)));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn finalize_before_inherents_fails() {
			let ctx = create_test_context().await;
			let header = create_next_header(&ctx.block, vec![]);

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
			builder.initialize().await.expect("initialize failed");

			// Try to finalize without applying inherents first
			let result = builder.finalize().await;
			assert!(matches!(result, Err(BlockBuilderError::InherentsNotApplied)));
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn finalize_produces_child_block() {
			use crate::inherent::TimestampInherent;

			let ctx = create_test_context().await;
			let parent_number = ctx.block.number;
			let parent_hash = ctx.block.hash;
			let header = create_next_header(&ctx.block, vec![]);

			// Create inherent providers - timestamp is required for finalization
			let providers: Vec<Box<dyn crate::InherentProvider>> =
				vec![Box::new(TimestampInherent::default_relay())];

			let mut builder =
				BlockBuilder::new(ctx.block, ctx.executor, header, providers, None, false);
			builder.initialize().await.expect("initialize failed");
			builder.apply_inherents().await.expect("apply_inherents failed");

			let (new_block, _prototype) = builder.finalize().await.expect("finalize failed");

			assert_eq!(new_block.number, parent_number + 1);
			assert_eq!(new_block.parent_hash, parent_hash);
			assert!(new_block.parent.is_some());
			assert!(!new_block.header.is_empty());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn create_next_header_increments_block_number() {
			let ctx = create_test_context().await;

			let header_bytes = create_next_header(&ctx.block, vec![]);

			// Header should not be empty
			assert!(!header_bytes.is_empty());

			// First 32 bytes should be the parent hash
			assert_eq!(&header_bytes[0..32], ctx.block.hash.as_bytes());
		}

		#[tokio::test(flavor = "multi_thread")]
		async fn create_next_header_includes_digest_items() {
			let ctx = create_test_context().await;

			// Create header with a PreRuntime digest item
			let slot: u64 = 12345;
			let digest_items = vec![DigestItem::PreRuntime(consensus_engine::AURA, slot.encode())];

			let header_bytes = create_next_header(&ctx.block, digest_items);

			// Header with digest should be larger than header without
			let empty_header = create_next_header(&ctx.block, vec![]);
			assert!(header_bytes.len() > empty_header.len());
		}
	}
}
