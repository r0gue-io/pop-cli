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
//!                 ┌──────────────┼──────────────┐
//!                 ▼              ▼              ▼
//!          ┌──────────┐   ┌──────────┐   ┌──────────┐
//!          │Timestamp │   │Parachain │   │  Future  │
//!          │ Inherent │   │ Inherent │   │Providers │
//!          └──────────┘   └──────────┘   └──────────┘
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
}
