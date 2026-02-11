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
use log::{debug, error, info};
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
		debug!("[BlockBuilder] {phase} storage: {stats}");
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
			debug!(
				"[BlockBuilder] Prefetched {} StorageValue + {} map keys ({} pallets, {} scans failed)",
				value_keys.len(),
				scan_keys,
				pallet_prefixes.len(),
				scan_errors,
			);
		} else {
			debug!(
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
			debug!("[BlockBuilder] Prefetching block building storage...");
			self.prefetch_block_building_storage().await?;
		}

		// Call Core_initialize_block with the header
		debug!("[BlockBuilder] Calling Core_initialize_block...");
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
		debug!("[BlockBuilder] Core_initialize_block OK");
		debug!(
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
			debug!("[BlockBuilder] Getting inherents from provider: {}", id);
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
						debug!(
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

			// Log with human-readable pallet/call when decodable.
			match self.decode_extrinsic_call(&extrinsic).await {
				Some(decoded) => {
					let mut msg = format!(
						"[BlockBuilder] Extrinsic included in block\n  \
						 Pallet: {}\n  \
						 Call:   {}\n  \
						 Hash:   0x{}",
						decoded.pallet,
						decoded.call,
						hex::encode(ext_hash),
					);
					for (name, value) in &decoded.args {
						msg.push_str(&format!("\n  {name}: {value}"));
					}
					info!("{msg}");
				},
				None => info!(
					"[BlockBuilder] Extrinsic included in block\n  \
					 Hash: 0x{}",
					hex::encode(ext_hash),
				),
			}

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
		debug!("[BlockBuilder] Calling BlockBuilder_finalize_block...");
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
		debug!("[BlockBuilder] BlockBuilder_finalize_block OK");

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

	/// Attempt to decode pallet name, call name, and arguments from raw
	/// extrinsic bytes. Returns `None` if decoding fails (never panics).
	///
	/// ## Extrinsic layout (v4)
	///
	/// ```text
	/// [compact_len] [version_byte] [signer+sig+extensions (signed only)] [pallet_idx call_idx args...]
	/// ```
	///
	/// For unsigned extrinsics the call starts at a fixed offset. For signed
	/// extrinsics the address and signature are parsed deterministically, then
	/// a short scan over the extensions area finds the call. Candidates are
	/// validated by checking the remaining bytes against the minimum encoded
	/// size of the call's arguments to reject false positives.
	async fn decode_extrinsic_call(&self, extrinsic: &[u8]) -> Option<DecodedCall> {
		let metadata = self.parent.metadata().await.ok()?;
		let remaining = strip_compact_prefix(extrinsic)?;

		let version_byte = *remaining.first()?;
		let is_signed = version_byte & 0x80 != 0;

		if !is_signed {
			let pi = *remaining.get(1)?;
			let ci = *remaining.get(2)?;
			return try_decode_call(&metadata, pi, ci, remaining.get(3..)?);
		}

		find_signed_call(&metadata, remaining)
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

// ---------------------------------------------------------------------------
// Extrinsic call decoding helpers
// ---------------------------------------------------------------------------

/// Decoded extrinsic call with pallet, call name, and arguments.
struct DecodedCall {
	pallet: String,
	call: String,
	args: Vec<(String, String)>,
}

/// Strip the SCALE compact length prefix, returning the remainder.
fn strip_compact_prefix(bytes: &[u8]) -> Option<&[u8]> {
	let mode = bytes.first()? & 0b11;
	match mode {
		0b00 => bytes.get(1..),
		0b01 => bytes.get(2..),
		0b10 => bytes.get(4..),
		_ => None,
	}
}

/// Parse the signed extrinsic header deterministically, then scan the
/// extensions area for a valid call. False positives are rejected because
/// `scale_value::scale::decode_as_type` must successfully consume each
/// argument field.
fn find_signed_call(metadata: &Metadata, remaining: &[u8]) -> Option<DecodedCall> {
	// Parse MultiAddress (version byte at offset 0, address variant at 1).
	let addr_variant = *remaining.get(1)?;
	let addr_data_len = match addr_variant {
		0x00 | 0x03 => 32, // Id / Address32
		0x04 => 20,        // Address20
		_ => return None,
	};
	let after_addr = 1 + 1 + addr_data_len; // version + variant + data

	// Candidate offsets for the start of extensions, trying both:
	//  - standard MultiSignature with variant byte (1 + 64 or 65)
	//  - mock format without variant byte (just 64 bytes)
	let sig_ends: [Option<usize>; 2] = [
		remaining.get(after_addr).and_then(|&v| match v {
			0x00 | 0x01 => Some(after_addr + 1 + 64),
			0x02 => Some(after_addr + 1 + 65),
			_ => None,
		}),
		Some(after_addr + 64),
	];

	// Extensions are typically 3-20 bytes; scan a generous window.
	const MAX_EXT_SCAN: usize = 30;

	for ext_start in sig_ends.into_iter().flatten() {
		let scan_end = (ext_start + MAX_EXT_SCAN).min(remaining.len().saturating_sub(2));
		for offset in ext_start..=scan_end {
			let pi = *remaining.get(offset)?;
			let ci = *remaining.get(offset + 1)?;
			if let Some(decoded) = try_decode_call(metadata, pi, ci, remaining.get(offset + 2..)?) {
				return Some(decoded);
			}
		}
	}

	None
}

/// Try to match `(pallet_index, call_index)` against metadata and validate
/// by fully decoding all argument fields with `scale_value`. If any field
/// fails to decode, this candidate is rejected.
fn try_decode_call(
	metadata: &Metadata,
	pallet_index: u8,
	call_index: u8,
	args_bytes: &[u8],
) -> Option<DecodedCall> {
	let pallet = metadata.pallets().find(|p| p.index() == pallet_index)?;
	let call = pallet.call_variants()?.iter().find(|v| v.index == call_index)?;

	let registry = metadata.types();
	let mut cursor: &[u8] = args_bytes;
	let mut args = Vec::new();

	for field in &call.fields {
		let value = scale_value::scale::decode_as_type(&mut cursor, field.ty.id, registry).ok()?;
		let name = field.name.as_deref().unwrap_or("?").to_string();
		let formatted = format_scale_value(&value)?;
		args.push((name, formatted));
	}

	// The call is at the very end of the extrinsic, so all bytes must be consumed.
	// Remaining bytes indicate a false positive match in the extensions area.
	if !cursor.is_empty() {
		return None;
	}

	Some(DecodedCall { pallet: pallet.name().to_string(), call: call.name.clone(), args })
}

/// Format byte sequences as UTF-8 strings when valid, otherwise as lowercase hex.
fn format_bytes<T, W: std::fmt::Write>(
	value: &scale_value::Value<T>,
	mut writer: W,
) -> Option<core::fmt::Result> {
	let mut hex_buf = String::new();
	let res = scale_value::stringify::custom_formatters::format_hex(value, &mut hex_buf);
	match res {
		Some(Ok(())) => {
			// format_hex recognized it as a byte sequence. Try UTF-8 first.
			let hex_str = hex_buf.trim_start_matches("0x");
			if let Ok(bytes) = hex::decode(hex_str) &&
				let Ok(s) = std::str::from_utf8(&bytes) &&
				!s.is_empty() &&
				s.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
			{
				return Some(writer.write_fmt(format_args!("\"{s}\"")));
			}
			Some(writer.write_str(&hex_buf.to_lowercase()))
		},
		other => other,
	}
}

/// Format a decoded `scale_value::Value` into a human-readable string.
/// Uses the built-in hex formatter so byte arrays render as `0x...`.
fn format_scale_value<T>(value: &scale_value::Value<T>) -> Option<String> {
	let mut buf = String::new();
	scale_value::stringify::to_writer_custom()
		.compact()
		.add_custom_formatter(|v, w| format_bytes(v, w))
		.write(value, &mut buf)
		.ok()?;
	Some(buf)
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
}
