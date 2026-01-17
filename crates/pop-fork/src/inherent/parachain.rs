// SPDX-License-Identifier: GPL-3.0

//! Parachain inherent provider for block building.
//!
//! This module provides [`ParachainInherent`] which generates the parachain
//! validation data inherent for parachain runtimes. This inherent is required
//! for parachains to validate blocks against the relay chain.
//!
//! # Current Limitations
//!
//! This is a basic/mock implementation that returns an empty vector.
//! Full parachain inherent support requires:
//!
//! - Relay chain state proofs
//! - Downward messages (DMP)
//! - Horizontal messages (HRMP)
//! - Validation code hash
//! - Relay parent number and storage root
//!
//! For local fork testing without relay chain interaction, the empty
//! implementation is sufficient as the inherent check can be bypassed
//! or mocked at the runtime level.
//!
//! # Future Work
//!
//! A full implementation would need to:
//! 1. Connect to the relay chain RPC
//! 2. Fetch validation data for the parachain
//! 3. Construct proper `PersistedValidationData` and `ValidationData`
//! 4. Encode as `parachainSystem.setValidationData(data)` call
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::inherent::ParachainInherent;
//!
//! // Create the provider (currently returns empty)
//! let provider = ParachainInherent::default();
//! ```

use crate::{
	Block, BlockBuilderError, RuntimeExecutor, inherent::InherentProvider,
	strings::inherent::parachain as strings,
};
use async_trait::async_trait;

/// Parachain inherent provider.
///
/// Generates the `parachainSystem.setValidationData` inherent extrinsic
/// that provides relay chain validation data to the parachain runtime.
///
/// # Current Implementation
///
/// Returns an empty vector, effectively making this a no-op. This is
/// suitable for testing scenarios where relay chain interaction is not
/// needed or is mocked at the runtime level.
///
/// # Detection
///
/// In future versions, this provider could detect whether the runtime
/// has the `parachainSystem` pallet and only generate inherents when
/// appropriate.
#[derive(Debug, Clone, Default)]
pub struct ParachainInherent {
	/// Parachain ID (reserved for future use).
	#[allow(dead_code)]
	para_id: Option<u32>,
}

impl ParachainInherent {
	/// Create a new parachain inherent provider.
	pub fn new() -> Self {
		Self::default()
	}

	/// Create a parachain inherent provider with a specific para ID.
	///
	/// The para ID is reserved for future use when full relay chain
	/// integration is implemented.
	#[allow(dead_code)]
	pub fn with_para_id(para_id: u32) -> Self {
		Self { para_id: Some(para_id) }
	}
}

#[async_trait]
impl InherentProvider for ParachainInherent {
	fn identifier(&self) -> &'static str {
		strings::IDENTIFIER
	}

	async fn provide(
		&self,
		_parent: &Block,
		_executor: &RuntimeExecutor,
	) -> Result<Vec<Vec<u8>>, BlockBuilderError> {
		// TODO: Implement full parachain validation data inherent.
		//
		// This would require:
		// 1. Checking if parachainSystem pallet exists in metadata
		// 2. Fetching validation data from relay chain
		// 3. Encoding the setValidationData call
		//
		// For now, return empty - suitable for:
		// - Relay chains (no parachain inherent needed)
		// - Testing scenarios without relay chain
		// - Runtimes with mocked/bypassed inherent checks
		Ok(vec![])
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_creates_provider_without_para_id() {
		let provider = ParachainInherent::default();
		assert!(provider.para_id.is_none());
	}

	#[test]
	fn with_para_id_sets_para_id() {
		let provider = ParachainInherent::with_para_id(1000);
		assert_eq!(provider.para_id, Some(1000));
	}

	#[test]
	fn identifier_returns_parachain_system() {
		let provider = ParachainInherent::default();
		assert_eq!(provider.identifier(), strings::IDENTIFIER);
	}
}
