// SPDX-License-Identifier: GPL-3.0

//! Timestamp inherent provider for block building.
//!
//! This module provides [`TimestampInherent`] which generates the mandatory
//! timestamp inherent extrinsic for each block. The timestamp pallet requires
//! this inherent to advance the chain's notion of time.
//!
//! # How It Works
//!
//! 1. Read the current timestamp from `Timestamp::Now` storage
//! 2. Add the configured slot duration
//! 3. Encode a `timestamp.set(new_timestamp)` call
//! 4. Wrap it as an unsigned inherent extrinsic
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::inherent::TimestampInherent;
//!
//! // Create with default 6-second slots (relay chain)
//! let provider = TimestampInherent::default_relay();
//!
//! // Create with custom slot duration
//! let provider = TimestampInherent::new(2_000); // 2 seconds
//! ```

use crate::{
	Block, BlockBuilderError, RuntimeExecutor, inherent::InherentProvider,
	strings::inherent::timestamp as strings,
};
use async_trait::async_trait;
use scale::{Compact, Decode, Encode};

/// Default slot duration for relay chains (6 seconds).
const DEFAULT_RELAY_SLOT_DURATION_MS: u64 = 6_000;

/// Default slot duration for parachains (6 seconds).
const DEFAULT_PARA_SLOT_DURATION_MS: u64 = 6_000;

/// Extrinsic format version for unsigned/bare extrinsics.
/// Version 5 is current; version 4 is legacy.
const EXTRINSIC_FORMAT_VERSION: u8 = 5;

// TODO: Pallet and call indices vary between runtimes. These hardcoded values work for
// Polkadot SDK template runtimes but will fail for runtimes with different pallet ordering.
// This should be improved by:
// 1. Parsing the runtime metadata to discover the actual pallet index for "Timestamp"
// 2. Looking up the call index for "set" within the timestamp pallet's calls
// See: https://docs.rs/frame-metadata for metadata parsing utilities.

/// Pallet index for timestamp in Polkadot SDK template runtimes.
/// Note: This index varies between runtimes and should ideally be read from metadata.
const TIMESTAMP_PALLET_INDEX: u8 = 3;

/// Call index for `timestamp.set` (typically the first/only call in the timestamp pallet).
/// Note: This index may vary between runtimes and should ideally be read from metadata.
const TIMESTAMP_SET_CALL_INDEX: u8 = 0;

/// Timestamp inherent provider.
///
/// Generates the `timestamp.set(now)` inherent extrinsic that advances
/// the chain's timestamp by the configured slot duration.
///
/// # Slot Duration
///
/// The slot duration determines how much time passes between blocks.
/// Both relay chains and parachains typically use 6-second slots.
///
/// # Pallet Index
///
/// The timestamp pallet index varies by runtime. This provider uses the
/// well-known index for Polkadot SDK template runtimes by default, but
/// allows configuration for other runtimes.
#[derive(Debug, Clone)]
pub struct TimestampInherent {
	/// Slot duration in milliseconds.
	slot_duration_ms: u64,
	/// Pallet index for the timestamp pallet.
	pallet_index: u8,
}

impl TimestampInherent {
	/// Create a new timestamp inherent provider.
	///
	/// # Arguments
	///
	/// * `slot_duration_ms` - Slot duration in milliseconds
	pub fn new(slot_duration_ms: u64) -> Self {
		Self { slot_duration_ms, pallet_index: Self::default_pallet_index() }
	}

	/// Create a timestamp inherent provider with a custom pallet index.
	///
	/// Use this if your runtime has the timestamp pallet at a non-standard index.
	///
	/// # Arguments
	///
	/// * `slot_duration_ms` - Slot duration in milliseconds
	/// * `pallet_index` - The index of the timestamp pallet in the runtime
	pub fn with_pallet_index(slot_duration_ms: u64, pallet_index: u8) -> Self {
		Self { slot_duration_ms, pallet_index }
	}

	/// Create with default settings for relay chains (6-second slots).
	pub fn default_relay() -> Self {
		Self::new(DEFAULT_RELAY_SLOT_DURATION_MS)
	}

	/// Create with default settings for parachains (6-second slots).
	pub fn default_para() -> Self {
		Self::new(DEFAULT_PARA_SLOT_DURATION_MS)
	}

	/// Get the pallet index for the timestamp pallet.
	///
	/// Returns the index commonly used in Polkadot SDK template runtimes.
	/// Use [`Self::with_pallet_index`] if your runtime uses a different index.
	///
	/// # Note
	///
	/// This should ideally be read from the runtime metadata instead of hardcoded.
	fn default_pallet_index() -> u8 {
		TIMESTAMP_PALLET_INDEX
	}

	/// Compute the storage key for `Timestamp::Now`.
	fn timestamp_now_key() -> Vec<u8> {
		let pallet_hash = sp_core::twox_128(strings::storage_keys::PALLET_NAME);
		let storage_hash = sp_core::twox_128(strings::storage_keys::NOW);
		[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
	}

	/// Encode the `timestamp.set(now)` call.
	///
	/// The call is encoded as: `[pallet_index, call_index, Compact<u64>]`
	/// where call_index is always 0 (the only call in the timestamp pallet).
	fn encode_timestamp_set_call(&self, timestamp: u64) -> Vec<u8> {
		let mut call = vec![self.pallet_index, TIMESTAMP_SET_CALL_INDEX];
		// Timestamp argument is encoded as Compact<u64>
		call.extend(Compact(timestamp).encode());
		call
	}

	/// Wrap a call as an unsigned inherent extrinsic.
	///
	/// Unsigned extrinsics have the format:
	/// - Compact length prefix
	/// - Version byte
	/// - Call data
	fn encode_inherent_extrinsic(call: Vec<u8>) -> Vec<u8> {
		let mut extrinsic = vec![EXTRINSIC_FORMAT_VERSION];
		extrinsic.extend(call);

		// Prefix with compact length
		let len = Compact(extrinsic.len() as u32);
		let mut result = len.encode();
		result.extend(extrinsic);
		result
	}
}

impl Default for TimestampInherent {
	fn default() -> Self {
		Self::default_relay()
	}
}

#[async_trait]
impl InherentProvider for TimestampInherent {
	fn identifier(&self) -> &'static str {
		strings::IDENTIFIER
	}

	async fn provide(
		&self,
		parent: &Block,
		_executor: &RuntimeExecutor,
	) -> Result<Vec<Vec<u8>>, BlockBuilderError> {
		// Read current timestamp from parent block storage
		let key = Self::timestamp_now_key();
		let storage = parent.storage();

		let current_timestamp = match storage.get(parent.number, &key).await? {
			Some(value) => u64::decode(&mut value.as_slice()).map_err(|e| {
				BlockBuilderError::InherentProvider {
					provider: self.identifier().to_string(),
					message: format!("{}: {}", strings::errors::DECODE_FAILED, e),
				}
			})?,
			None => {
				// No timestamp set yet (genesis or very early block)
				// Use current system time as fallback
				std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.map(|d| d.as_millis() as u64)
					.unwrap_or(0)
			},
		};

		// Calculate new timestamp
		let new_timestamp = current_timestamp.saturating_add(self.slot_duration_ms);

		// Encode the timestamp.set call
		let call = self.encode_timestamp_set_call(new_timestamp);

		// Wrap as unsigned extrinsic
		let extrinsic = Self::encode_inherent_extrinsic(call);

		Ok(vec![extrinsic])
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Custom slot duration for testing (1 second).
	const TEST_SLOT_DURATION_MS: u64 = 1_000;

	/// Custom pallet index for testing.
	const TEST_PALLET_INDEX: u8 = 42;

	#[test]
	fn new_creates_provider_with_slot_duration() {
		let provider = TimestampInherent::new(TEST_SLOT_DURATION_MS);
		assert_eq!(provider.slot_duration_ms, TEST_SLOT_DURATION_MS);
	}

	#[test]
	fn default_relay_uses_configured_slot_duration() {
		let provider = TimestampInherent::default_relay();
		assert_eq!(provider.slot_duration_ms, DEFAULT_RELAY_SLOT_DURATION_MS);
	}

	#[test]
	fn default_para_uses_configured_slot_duration() {
		let provider = TimestampInherent::default_para();
		assert_eq!(provider.slot_duration_ms, DEFAULT_PARA_SLOT_DURATION_MS);
	}

	#[test]
	fn with_pallet_index_sets_custom_index() {
		let provider =
			TimestampInherent::with_pallet_index(DEFAULT_RELAY_SLOT_DURATION_MS, TEST_PALLET_INDEX);
		assert_eq!(provider.pallet_index, TEST_PALLET_INDEX);
	}

	#[test]
	fn timestamp_now_key_is_32_bytes() {
		let key = TimestampInherent::timestamp_now_key();
		// twox128 produces 16 bytes per hash, storage key = pallet hash + item hash
		const TWOX128_OUTPUT_BYTES: usize = 16;
		const STORAGE_KEY_LEN: usize = TWOX128_OUTPUT_BYTES * 2;
		assert_eq!(key.len(), STORAGE_KEY_LEN);
	}

	#[test]
	fn encode_timestamp_set_call_produces_valid_encoding() {
		let provider = TimestampInherent::new(DEFAULT_RELAY_SLOT_DURATION_MS);
		let call = provider.encode_timestamp_set_call(1_000_000);

		// First byte is pallet index
		assert_eq!(call[0], TIMESTAMP_PALLET_INDEX);
		// Second byte is call index
		assert_eq!(call[1], TIMESTAMP_SET_CALL_INDEX);
		// Rest is compact-encoded timestamp
		assert!(call.len() > 2);
	}

	#[test]
	fn encode_inherent_extrinsic_includes_version_and_length() {
		// Create fake call data using actual constants
		let call = vec![TIMESTAMP_PALLET_INDEX, TIMESTAMP_SET_CALL_INDEX, 1, 2, 3];
		let extrinsic = TimestampInherent::encode_inherent_extrinsic(call.clone());

		// Should start with compact length (6 = version byte + 5 call bytes)
		// Compact encoding of 6 is (6 << 2) = 0x18
		const EXPECTED_COMPACT_LEN: u8 = 0x18;
		assert_eq!(extrinsic[0], EXPECTED_COMPACT_LEN);
		// Next byte is extrinsic format version
		assert_eq!(extrinsic[1], EXTRINSIC_FORMAT_VERSION);
		// Rest is the call
		assert_eq!(&extrinsic[2..], &call[..]);
	}

	#[test]
	fn identifier_returns_timestamp() {
		let provider = TimestampInherent::default();
		assert_eq!(provider.identifier(), strings::IDENTIFIER);
	}
}
