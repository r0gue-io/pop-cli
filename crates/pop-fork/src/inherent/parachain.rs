// SPDX-License-Identifier: GPL-3.0

//! Parachain inherent provider for block building.
//!
//! This module provides [`ParachainInherent`] which generates the parachain
//! validation data inherent for parachain runtimes. This inherent is required
//! for parachains to validate blocks against the relay chain.
//!
//! # How It Works
//!
//! For parachains, the `setValidationData` inherent must be applied before
//! other inherents (like timestamp). This provider:
//!
//! 1. Finds the `setValidationData` extrinsic in the parent block
//! 2. Extracts and decodes the validation data
//! 3. Increments the relay parent number
//! 4. Re-encodes the extrinsic with the updated data
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::inherent::ParachainInherent;
//!
//! let provider = ParachainInherent::new();
//! ```

use crate::{
	Block, BlockBuilderError, RuntimeExecutor, inherent::InherentProvider,
	strings::inherent::parachain as strings,
};
use async_trait::async_trait;
use scale::{Compact, Decode, Encode};

/// Extrinsic format version for unsigned/bare extrinsics.
const EXTRINSIC_FORMAT_VERSION: u8 = 5;

/// Parachain inherent provider.
///
/// Generates the `parachainSystem.setValidationData` inherent extrinsic
/// that provides relay chain validation data to the parachain runtime.
///
/// # Implementation
///
/// This provider extracts the validation data from the parent block's
/// `setValidationData` extrinsic and updates the relay parent number.
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

	/// Compute the storage key for `ParachainSystem::LastRelayChainBlockNumber`.
	fn last_relay_chain_block_number_key() -> Vec<u8> {
		let pallet_hash = sp_core::twox_128(b"ParachainSystem");
		let storage_hash = sp_core::twox_128(b"LastRelayChainBlockNumber");
		[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
	}

	/// Read the last relay chain block number from storage.
	async fn read_last_relay_chain_block_number(parent: &Block) -> Option<u32> {
		let key = Self::last_relay_chain_block_number_key();
		let storage = parent.storage();

		match storage.get(parent.number, &key).await {
			Ok(Some(entry)) if entry.value.is_some() => {
				let bytes = entry.value.as_ref()?;
				u32::decode(&mut &bytes[..]).ok()
			},
			_ => None,
		}
	}

	/// Find and extract the setValidationData extrinsic from the parent block.
	///
	/// Returns the raw extrinsic bytes and the offset where validation data starts.
	fn find_validation_data_extrinsic(
		extrinsics: &[Vec<u8>],
		pallet_index: u8,
		call_index: u8,
	) -> Option<&Vec<u8>> {
		for ext in extrinsics {
			// Skip compact length prefix
			let (_, remainder) = decode_compact_len(ext)?;

			// Check version byte (should be 0x05 for unsigned v5)
			if remainder.first() != Some(&EXTRINSIC_FORMAT_VERSION) {
				continue;
			}

			// Check pallet and call indices
			if remainder.len() >= 3 && remainder[1] == pallet_index && remainder[2] == call_index {
				return Some(ext);
			}
		}
		None
	}

	/// Find the offset of a known u32 value in the extrinsic bytes.
	///
	/// This searches for the little-endian encoding of the value in the
	/// extrinsic body (after the compact length prefix).
	fn find_u32_offset_in_body(body: &[u8], value: u32) -> Option<usize> {
		let needle = value.to_le_bytes();
		// Start searching after version + pallet + call (3 bytes)
		for i in 3..body.len().saturating_sub(3) {
			if body[i..].starts_with(&needle) {
				return Some(i);
			}
		}
		None
	}

	/// Patch the relay parent number in the validation data extrinsic.
	///
	/// Uses the known relay chain block number from storage to find the
	/// correct offset in the extrinsic, then increments it.
	fn patch_relay_parent_number(
		extrinsic: &[u8],
		known_relay_block: u32,
		increment: u32,
	) -> Option<Vec<u8>> {
		// Decode compact length prefix
		let (compact_len, body) = decode_compact_len(extrinsic)?;

		// Find the offset of the known relay block number in the extrinsic body
		let relay_parent_offset = Self::find_u32_offset_in_body(body, known_relay_block)?;

		eprintln!(
			"[ParachainInherent] Found relay_parent_number {} at offset {}",
			known_relay_block, relay_parent_offset
		);

		// Calculate new number
		let new_number = known_relay_block.saturating_add(increment);

		eprintln!(
			"[ParachainInherent] Patching relay parent number: {} -> {}",
			known_relay_block, new_number
		);

		// Create new extrinsic with patched number
		let mut new_body = body.to_vec();
		new_body[relay_parent_offset..relay_parent_offset + 4]
			.copy_from_slice(&new_number.to_le_bytes());

		// Re-encode with compact length prefix
		// Note: since we're only changing a fixed-size field, the length doesn't change
		let mut result = encode_compact_len(compact_len);
		result.extend(new_body);

		Some(result)
	}
}

/// Decode a compact length prefix from SCALE-encoded data.
/// Returns (length_value, remaining_bytes).
fn decode_compact_len(data: &[u8]) -> Option<(u32, &[u8])> {
	if data.is_empty() {
		return None;
	}

	let first_byte = data[0];
	let mode = first_byte & 0b11;

	match mode {
		0b00 => {
			// Single byte mode: 6 bits of data
			let len = (first_byte >> 2) as u32;
			Some((len, &data[1..]))
		},
		0b01 => {
			// Two byte mode: 14 bits of data
			if data.len() < 2 {
				return None;
			}
			let len = (u16::from_le_bytes([data[0], data[1]]) >> 2) as u32;
			Some((len, &data[2..]))
		},
		0b10 => {
			// Four byte mode: 30 bits of data
			if data.len() < 4 {
				return None;
			}
			let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) >> 2;
			Some((len, &data[4..]))
		},
		0b11 => {
			// Big integer mode (not typically used for extrinsic lengths)
			None
		},
		_ => None,
	}
}

/// Encode a u32 as a compact length prefix.
fn encode_compact_len(value: u32) -> Vec<u8> {
	Compact(value).encode()
}

#[async_trait]
impl InherentProvider for ParachainInherent {
	fn identifier(&self) -> &'static str {
		strings::IDENTIFIER
	}

	async fn provide(
		&self,
		parent: &Block,
		_executor: &RuntimeExecutor,
	) -> Result<Vec<Vec<u8>>, BlockBuilderError> {
		// Check if ParachainSystem pallet exists in metadata.
		// If not, this is a relay chain or standalone chain - gracefully skip.
		let metadata = parent.metadata().await?;

		let pallet = match metadata.pallet_by_name(strings::metadata::PALLET_NAME) {
			Some(p) => p,
			None => {
				// No ParachainSystem pallet - this is not a parachain runtime
				return Ok(vec![]);
			},
		};

		let pallet_index = pallet.index();

		// Get the call index for setValidationData
		let call_variant = pallet
			.call_variant_by_name(strings::metadata::SET_VALIDATION_DATA_CALL_NAME)
			.ok_or_else(|| BlockBuilderError::InherentProvider {
				provider: self.identifier().to_string(),
				message: format!(
					"Call '{}' not found in pallet '{}'",
					strings::metadata::SET_VALIDATION_DATA_CALL_NAME,
					strings::metadata::PALLET_NAME
				),
			})?;

		let call_index = call_variant.index;

		eprintln!(
			"[ParachainInherent] Looking for setValidationData: pallet={}, call={}",
			pallet_index, call_index
		);
		eprintln!("[ParachainInherent] Parent block has {} extrinsics", parent.extrinsics.len());

		// Read the last relay chain block number from storage
		let last_relay_block = Self::read_last_relay_chain_block_number(parent).await;
		eprintln!(
			"[ParachainInherent] lastRelayChainBlockNumber from storage: {:?}",
			last_relay_block
		);

		// Find the setValidationData extrinsic in the parent block
		let validation_ext =
			Self::find_validation_data_extrinsic(&parent.extrinsics, pallet_index, call_index);

		match (validation_ext, last_relay_block) {
			(Some(ext), Some(relay_block)) => {
				eprintln!(
					"[ParachainInherent] Found setValidationData extrinsic ({} bytes)",
					ext.len()
				);

				// Patch the relay parent number (increment by 1)
				match Self::patch_relay_parent_number(ext, relay_block, 1) {
					Some(patched) => {
						eprintln!(
							"[ParachainInherent] Patched extrinsic ({} bytes)",
							patched.len()
						);
						Ok(vec![patched])
					},
					None => {
						eprintln!(
							"[ParachainInherent] Failed to find relay block {} in extrinsic",
							relay_block
						);
						// Can't patch - return empty and let block building fail with clear error
						Ok(vec![])
					},
				}
			},
			(Some(ext), None) => {
				eprintln!(
					"[ParachainInherent] Found extrinsic but no storage value, using original"
				);
				// No storage value but have extrinsic - try using it as-is
				Ok(vec![ext.clone()])
			},
			(None, _) => {
				eprintln!(
					"[ParachainInherent] No setValidationData extrinsic found in parent block"
				);
				// No validation data in parent - this might be the first block after fork
				// For now, return empty and let the timestamp inherent try without it
				// This will likely fail, but provides a clear error message
				Ok(vec![])
			},
		}
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

	#[test]
	fn decode_compact_len_single_byte() {
		// 6 << 2 = 24 (0x18) encodes length 6 in single-byte mode
		let data = [0x18, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
		let (len, remainder) = decode_compact_len(&data).unwrap();
		assert_eq!(len, 6);
		assert_eq!(remainder.len(), 6);
	}

	#[test]
	fn decode_compact_len_two_byte() {
		// 100 << 2 | 0b01 = 0x191 encodes length 100 in two-byte mode
		let data = [0x91, 0x01, 0x00, 0x00]; // LE encoding
		let (len, remainder) = decode_compact_len(&data).unwrap();
		assert_eq!(len, 100);
		assert_eq!(remainder.len(), 2);
	}

	#[test]
	fn encode_compact_len_single_byte() {
		let encoded = encode_compact_len(6);
		assert_eq!(encoded, vec![0x18]); // 6 << 2 = 24
	}

	#[test]
	fn find_validation_data_extrinsic_finds_matching() {
		// Create a mock extrinsic: compact_len + version + pallet + call + data
		let mut ext = encode_compact_len(10); // length prefix
		ext.push(EXTRINSIC_FORMAT_VERSION); // version
		ext.push(51); // pallet index
		ext.push(0); // call index
		ext.extend([0u8; 7]); // padding to reach length

		let extrinsics = vec![ext.clone()];
		let result = ParachainInherent::find_validation_data_extrinsic(&extrinsics, 51, 0);
		assert!(result.is_some());
	}

	#[test]
	fn find_validation_data_extrinsic_ignores_non_matching() {
		// Create a mock extrinsic with different pallet/call
		let mut ext = encode_compact_len(5);
		ext.push(EXTRINSIC_FORMAT_VERSION);
		ext.push(10); // different pallet
		ext.push(5); // different call
		ext.extend([0u8; 2]);

		let extrinsics = vec![ext];
		let result = ParachainInherent::find_validation_data_extrinsic(&extrinsics, 51, 0);
		assert!(result.is_none());
	}
}
