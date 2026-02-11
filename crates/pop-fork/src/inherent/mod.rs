// SPDX-License-Identifier: GPL-3.0

//! Inherent extrinsic providers for block building.
//!
//! This module defines the [`InherentProvider`] trait and provides implementations
//! for generating inherent extrinsics during block construction.
//!
//! # What are Inherents?
//!
//! Inherent are special transactions that:
//! - Are unsigned (no signature required)
//! - Are mandatory (block is invalid without them)
//! - Are applied before regular extrinsics
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    InherentProvider Trait                       │
//! └─────────────────────────────────────────────────────────────────┘
//!                                │
//!          ┌─────────────────────┼─────────────────────┐
//!          ▼                     ▼                     ▼
//!    ┌──────────┐          ┌──────────┐          ┌──────────┐
//!    │Timestamp │          │Parachain │          │RelayChain│
//!    │ Inherent │          │ Inherent │          │ Inherent │
//!    └──────────┘          └──────────┘          └──────────┘
//! ```
//!
//! # Implementing a Provider
//!
//! ```ignore
//! use pop_fork::{InherentProvider, Block, BlockBuilderError, RuntimeExecutor};
//! use async_trait::async_trait;
//!
//! pub struct TimestampInherent {
//!     slot_duration_ms: u64,
//! }
//!
//! #[async_trait]
//! impl InherentProvider for TimestampInherent {
//!     fn identifier(&self) -> &'static str {
//!         "Timestamp"
//!     }
//!
//!     async fn provide(
//!         &self,
//!         parent: &Block,
//!         executor: &RuntimeExecutor,
//!     ) -> Result<Vec<Vec<u8>>, BlockBuilderError> {
//!         // Read current timestamp, add slot_duration, encode call
//!         Ok(vec![encoded_timestamp_set_call])
//!     }
//! }
//! ```

mod parachain;
pub mod relay;
mod relay_proof;
pub mod slot;
mod timestamp;

pub use parachain::ParachainInherent;
pub use relay::{PARA_INHERENT_PALLET, para_inherent_included_key};
pub use relay_proof::{
	CURRENT_SLOT_KEY, ProofError, modify_proof, paras_heads_key, read_from_proof,
	read_raw_from_proof,
};
pub use slot::{
	ConsensusType, aura_current_slot_key, babe_current_slot_key, calculate_next_slot,
	detect_consensus_type, encode_aura_slot, encode_babe_predigest,
};
pub use timestamp::TimestampInherent;

use crate::{Block, BlockBuilderError, RuntimeExecutor};
use async_trait::async_trait;

/// Trait for creating inherent extrinsics during block building.
///
/// Inherent providers generate the "inherent" extrinsics that are automatically
/// included in each block (timestamp, parachain validation data, etc.).
///
/// # Implementing
///
/// Implementations should return an empty `Vec` if the inherent doesn't apply
/// to the current chain (e.g., parachain inherents on a relay chain).
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

	/// Pre-cache expensive computations to speed up the first `provide()` call.
	///
	/// Called during background warmup after forking. The default implementation
	/// is a no-op. Providers that perform expensive runtime calls (e.g. WASM
	/// execution to detect slot duration) should override this to cache the result.
	async fn warmup(&self, _parent: &Block, _executor: &RuntimeExecutor) {}

	/// Invalidate cached runtime-derived values.
	///
	/// Called after a runtime upgrade so that providers re-detect values
	/// (e.g. slot duration) from the new runtime on the next `provide()` call.
	/// The default implementation is a no-op.
	fn invalidate_cache(&self) {}
}

/// Create default inherent providers for block building.
///
/// This factory function creates a standard set of inherent providers
/// suitable for most chains.
///
/// # Arguments
///
/// * `is_parachain` - Whether the chain is a parachain (affects slot duration and adds parachain
///   inherent provider)
///
/// # Slot Duration
///
/// The timestamp inherent provider uses default slot durations:
/// - Relay chains: 6 seconds (6000ms)
/// - Parachains: 12 seconds (12000ms)
///
/// The actual slot duration will be detected from the runtime at block-building time,
/// with these values serving as fallbacks.
///
/// # Returns
///
/// A vector of boxed inherent providers ready for use with `BlockBuilder`.
///
/// # Example
///
/// ```ignore
/// use pop_fork::inherent::default_providers;
///
/// // For a relay chain
/// let providers = default_providers(false);
///
/// // For a parachain
/// let providers = default_providers(true);
/// ```
pub fn default_providers(is_parachain: bool) -> Vec<Box<dyn InherentProvider>> {
	let timestamp = if is_parachain {
		TimestampInherent::default_para()
	} else {
		TimestampInherent::default_relay()
	};

	// For parachains, setValidationData MUST be applied BEFORE timestamp.
	// The validation data sets up the relay chain state that timestamp pallet
	// uses for time validation checks.
	//
	// For relay chains, the ParaInherent::Included storage is mocked automatically
	// by BlockBuilder::apply_inherents() - no provider needed.
	if is_parachain {
		vec![Box::new(ParachainInherent::new()), Box::new(timestamp)]
	} else {
		vec![Box::new(timestamp)]
	}
}
