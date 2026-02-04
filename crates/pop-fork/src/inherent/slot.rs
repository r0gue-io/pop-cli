// SPDX-License-Identifier: GPL-3.0

//! Slot calculation utilities for block building.
//!
//! This module provides utilities for computing and managing consensus slots
//! during block construction. It supports both Aura and Babe consensus mechanisms.
//!
//! # How It Works
//!
//! 1. Detect the consensus type from runtime metadata (Aura, Babe, or Unknown)
//! 2. Read the current timestamp from storage
//! 3. Get the slot duration from the runtime
//! 4. Calculate the next slot: `(current_timestamp + slot_duration) / slot_duration`
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::inherent::slot::{ConsensusType, detect_consensus_type, calculate_next_slot};
//!
//! let consensus = detect_consensus_type(&metadata);
//! if consensus != ConsensusType::Unknown {
//!     let next_slot = calculate_next_slot(current_timestamp, slot_duration);
//! }
//! ```

use scale::Encode;
use subxt::Metadata;

/// Consensus engine type detected from runtime metadata.
///
/// This enum represents the slot-based consensus mechanism used by the chain.
/// It is detected by checking which consensus pallet exists in the runtime metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsensusType {
	/// Aura (Authority Round) consensus.
	///
	/// Uses `PreRuntime(*b"aura", slot)` digest items.
	Aura,
	/// Babe (Blind Assignment for Blockchain Extension) consensus.
	///
	/// Uses `PreRuntime(*b"BABE", slot)` digest items.
	Babe,
	/// Unknown or no slot-based consensus.
	///
	/// The chain either doesn't use slots or uses a consensus mechanism
	/// that we don't recognize.
	Unknown,
}

/// Pallet name for Aura consensus.
const AURA_PALLET: &str = "Aura";

/// Pallet name for Babe consensus.
const BABE_PALLET: &str = "Babe";

/// Compute the storage key for `Aura::CurrentSlot`.
///
/// The key is constructed as: `twox_128("Aura") ++ twox_128("CurrentSlot")`
///
/// # Returns
///
/// A 32-byte storage key.
pub fn aura_current_slot_key() -> Vec<u8> {
	let pallet_hash = sp_core::twox_128(b"Aura");
	let storage_hash = sp_core::twox_128(b"CurrentSlot");
	[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
}

/// Compute the storage key for `Babe::CurrentSlot`.
///
/// The key is constructed as: `twox_128("Babe") ++ twox_128("CurrentSlot")`
///
/// # Returns
///
/// A 32-byte storage key.
pub fn babe_current_slot_key() -> Vec<u8> {
	let pallet_hash = sp_core::twox_128(b"Babe");
	let storage_hash = sp_core::twox_128(b"CurrentSlot");
	[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
}

/// Detect the consensus type from runtime metadata.
///
/// This function checks for the presence of Aura or Babe pallets in the
/// runtime metadata to determine which consensus mechanism the chain uses.
///
/// # Arguments
///
/// * `metadata` - The runtime metadata to inspect
///
/// # Returns
///
/// The detected consensus type. Returns `ConsensusType::Unknown` if neither
/// Aura nor Babe pallets are found.
///
/// # Detection Order
///
/// 1. Check for `Aura` pallet -> `ConsensusType::Aura`
/// 2. Check for `Babe` pallet -> `ConsensusType::Babe`
/// 3. Otherwise -> `ConsensusType::Unknown`
pub fn detect_consensus_type(metadata: &Metadata) -> ConsensusType {
	if metadata.pallet_by_name(AURA_PALLET).is_some() {
		ConsensusType::Aura
	} else if metadata.pallet_by_name(BABE_PALLET).is_some() {
		ConsensusType::Babe
	} else {
		ConsensusType::Unknown
	}
}

/// Calculate the next slot number based on current timestamp and slot duration.
///
/// The formula is: `next_slot = (current_timestamp + slot_duration) / slot_duration`
///
/// This effectively computes what slot the next block will be in, given that
/// we're advancing time by one slot duration.
///
/// # Arguments
///
/// * `current_timestamp_ms` - The current timestamp in milliseconds
/// * `slot_duration_ms` - The slot duration in milliseconds
///
/// # Returns
///
/// The next slot number.
///
/// # Panics
///
/// Panics if `slot_duration_ms` is zero.
///
/// # Example
///
/// ```ignore
/// // Current timestamp: 12000ms, slot duration: 6000ms
/// // Next timestamp: 18000ms, next slot: 18000 / 6000 = 3
/// let next_slot = calculate_next_slot(12_000, 6_000);
/// assert_eq!(next_slot, 3);
/// ```
pub fn calculate_next_slot(current_timestamp_ms: u64, slot_duration_ms: u64) -> u64 {
	assert!(slot_duration_ms > 0, "Slot duration cannot be zero");
	let next_timestamp = current_timestamp_ms.saturating_add(slot_duration_ms);
	next_timestamp / slot_duration_ms
}

/// Encode an Aura slot for use in a PreRuntime digest.
///
/// For Aura, the slot is simply encoded as a u64.
///
/// # Arguments
///
/// * `slot` - The slot number
///
/// # Returns
///
/// The SCALE-encoded slot bytes.
pub fn encode_aura_slot(slot: u64) -> Vec<u8> {
	slot.encode()
}

/// Encode a Babe PreDigest for use in a PreRuntime digest.
///
/// This creates a `SecondaryPlain` PreDigest which doesn't require VRF.
/// The format is suitable for forked execution where we don't have access
/// to the real block author's VRF keys.
///
/// # Babe PreDigest Format
///
/// ```text
/// enum PreDigest {
///     Primary(PrimaryPreDigest),           // index 1
///     SecondaryPlain(SecondaryPlainPreDigest), // index 2
///     SecondaryVRF(SecondaryVRFPreDigest), // index 3
/// }
///
/// struct SecondaryPlainPreDigest {
///     authority_index: u32,
///     slot: Slot,  // u64
/// }
/// ```
///
/// # Arguments
///
/// * `slot` - The slot number
/// * `authority_index` - The authority index (typically 0 for forked execution)
///
/// # Returns
///
/// The SCALE-encoded PreDigest bytes.
pub fn encode_babe_predigest(slot: u64, authority_index: u32) -> Vec<u8> {
	// SecondaryPlain variant has index 2 in the PreDigest enum
	// But SCALE enum encoding uses the actual variant index from the Rust enum definition
	// Looking at sp_consensus_babe::digests::PreDigest:
	// - Primary = 1
	// - SecondaryPlain = 2
	// - SecondaryVRF = 3
	//
	// SCALE encodes enums as: variant_index (1 byte) + variant_data
	const SECONDARY_PLAIN_INDEX: u8 = 2;

	let mut encoded = vec![SECONDARY_PLAIN_INDEX];
	// SecondaryPlainPreDigest: authority_index (u32) + slot (u64)
	encoded.extend(authority_index.encode());
	encoded.extend(slot.encode());
	encoded
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Test slot calculation with typical values.
	#[test]
	fn calculate_next_slot_works_correctly() {
		// timestamp=12000, duration=6000 -> next_timestamp=18000, slot=3
		assert_eq!(calculate_next_slot(12_000, 6_000), 3);

		// timestamp=0, duration=6000 -> next_timestamp=6000, slot=1
		assert_eq!(calculate_next_slot(0, 6_000), 1);

		// timestamp=5999, duration=6000 -> next_timestamp=11999, slot=1
		assert_eq!(calculate_next_slot(5_999, 6_000), 1);

		// timestamp=6000, duration=6000 -> next_timestamp=12000, slot=2
		assert_eq!(calculate_next_slot(6_000, 6_000), 2);
	}

	/// Test slot calculation with parachain slot duration (12 seconds).
	#[test]
	fn calculate_next_slot_with_parachain_duration() {
		// Parachain: 12-second slots
		const PARA_SLOT_DURATION: u64 = 12_000;

		// timestamp=0 -> next_slot=1
		assert_eq!(calculate_next_slot(0, PARA_SLOT_DURATION), 1);

		// timestamp=12000 -> next_slot=2
		assert_eq!(calculate_next_slot(12_000, PARA_SLOT_DURATION), 2);

		// timestamp=24000 -> next_slot=3
		assert_eq!(calculate_next_slot(24_000, PARA_SLOT_DURATION), 3);
	}

	/// Test slot calculation handles saturation correctly.
	#[test]
	fn calculate_next_slot_saturates_on_overflow() {
		// Very large timestamp near u64::MAX
		let large_timestamp = u64::MAX - 1000;
		// Should saturate to u64::MAX, not overflow
		let result = calculate_next_slot(large_timestamp, 6_000);
		// Result should be u64::MAX / 6000 (since timestamp saturates)
		assert_eq!(result, u64::MAX / 6_000);
	}

	/// Test that zero slot duration panics.
	#[test]
	#[should_panic(expected = "Slot duration cannot be zero")]
	fn calculate_next_slot_panics_on_zero_duration() {
		calculate_next_slot(12_000, 0);
	}

	/// Test Aura slot encoding is just the u64.
	#[test]
	fn encode_aura_slot_produces_u64_le() {
		let slot: u64 = 12345;
		let encoded = encode_aura_slot(slot);

		// Should be 8 bytes (u64 little-endian)
		assert_eq!(encoded.len(), 8);
		assert_eq!(encoded, slot.to_le_bytes());
	}

	/// Test Babe PreDigest encoding format.
	#[test]
	fn encode_babe_predigest_produces_correct_format() {
		let slot: u64 = 295033271;
		let authority_index: u32 = 0;
		let encoded = encode_babe_predigest(slot, authority_index);

		// Format: variant_index (1) + authority_index (4) + slot (8) = 13 bytes
		assert_eq!(encoded.len(), 13);

		// First byte is SecondaryPlain variant index (2)
		assert_eq!(encoded[0], 2);

		// Next 4 bytes are authority_index (u32 LE)
		assert_eq!(&encoded[1..5], &authority_index.to_le_bytes());

		// Last 8 bytes are slot (u64 LE)
		assert_eq!(&encoded[5..13], &slot.to_le_bytes());
	}

	/// Test Babe PreDigest with non-zero authority index.
	#[test]
	fn encode_babe_predigest_with_authority_index() {
		let slot: u64 = 100;
		let authority_index: u32 = 5;
		let encoded = encode_babe_predigest(slot, authority_index);

		// Verify authority index is encoded correctly
		assert_eq!(&encoded[1..5], &5u32.to_le_bytes());
	}
}
