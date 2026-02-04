// SPDX-License-Identifier: GPL-3.0

//! Relay chain storage mocking for block building.
//!
//! This module provides utilities for mocking relay chain-specific storage
//! that is required for block finalization. On relay chains, the `ParaInherent`
//! pallet requires that `Included` storage is set every block.
//!
//! # How It Works
//!
//! The relay chain runtime's `ParaInherent` pallet has an `on_finalize` hook that
//! panics if `Included` storage is not set:
//!
//! ```ignore
//! fn on_finalize(_: BlockNumberFor<T>) {
//!     if Included::<T>::take().is_none() {
//!         panic!("Bitfields and heads must be included every block");
//!     }
//! }
//! ```
//!
//! Instead of constructing a complex `paras_inherent.enter` extrinsic with proper
//! bitfields and candidates, we simply mock the `Included` storage directly.
//! The runtime only checks for existence, not validity.
//!
//! # Usage
//!
//! This is handled automatically by `BlockBuilder::apply_inherents()` when it
//! detects a relay chain runtime (one with `ParaInherent` pallet).

/// Pallet name for ParaInherent (relay chain parachains inherent).
pub const PARA_INHERENT_PALLET: &str = "ParaInherent";

/// Compute the storage key for `ParaInherent::Included`.
///
/// The key is constructed as: `twox_128("ParaInherent") ++ twox_128("Included")`
///
/// This storage value is checked in `on_finalize` to ensure the paras inherent
/// was "included" in the block. By setting this directly, we bypass the need
/// for a valid `paras_inherent.enter` extrinsic.
///
/// # Returns
///
/// A 32-byte storage key.
pub fn para_inherent_included_key() -> Vec<u8> {
	let pallet_hash = sp_core::twox_128(b"ParaInherent");
	let storage_hash = sp_core::twox_128(b"Included");
	[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn para_inherent_included_key_is_32_bytes() {
		let key = para_inherent_included_key();
		// twox_128 produces 16 bytes, so pallet + storage = 32 bytes
		assert_eq!(key.len(), 32);
	}

	#[test]
	fn para_inherent_included_key_is_deterministic() {
		let key1 = para_inherent_included_key();
		let key2 = para_inherent_included_key();
		assert_eq!(key1, key2);
	}
}
