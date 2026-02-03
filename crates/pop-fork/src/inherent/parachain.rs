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
	proof::{self, well_known_keys},
	strings::inherent::parachain as strings,
};
use async_trait::async_trait;
use scale::{Compact, Decode, Encode};
use std::collections::BTreeMap;

/// SCALE compact encoding modes.
///
/// The 2 least significant bits of the first byte indicate the encoding mode.
/// See: https://docs.substrate.io/reference/scale-codec/
mod compact_mode {
	/// Single byte mode: value is stored in bits 2-7 (6 bits, max 63).
	pub const SINGLE_BYTE: u8 = 0b00;
	/// Two byte mode: value is stored in bits 2-15 (14 bits, max 16383).
	pub const TWO_BYTE: u8 = 0b01;
	/// Four byte mode: value is stored in bits 2-31 (30 bits, max ~1 billion).
	pub const FOUR_BYTE: u8 = 0b10;
	/// Big integer mode: remaining bytes encode the value.
	pub const BIG_INTEGER: u8 = 0b11;

	/// Bitmask to extract the mode from the first byte.
	pub const MODE_MASK: u8 = 0b11;
	/// Number of bits to shift to get the value in single-byte mode.
	pub const VALUE_SHIFT: u8 = 2;
}

/// Extrinsic format versions and types.
mod extrinsic_format {
	/// V4 bare/unsigned extrinsic version byte.
	pub const BARE_V4: u8 = 0x04;
	/// V5 bare extrinsic version byte.
	/// V5 encodes both version and type: 2 MSB = type (00=bare), 6 LSB = version (5).
	pub const BARE_V5: u8 = 0x05;
}

/// Field sizes in bytes for ParachainInherentData structure.
#[allow(dead_code)]
mod field_sizes {
	/// Size of relay_parent_number (u32).
	pub const RELAY_PARENT_NUMBER: usize = 4;
	/// Size of relay_parent_storage_root (H256).
	pub const STORAGE_ROOT: usize = 32;
	/// Size of max_pov_size (u32).
	pub const MAX_POV_SIZE: usize = 4;
}

/// Check if a version byte indicates a bare/unsigned extrinsic.
///
/// Accepts both V4 (0x04) and V5 (0x05) bare extrinsic formats.
fn is_bare_extrinsic(version_byte: u8) -> bool {
	version_byte == extrinsic_format::BARE_V4 || version_byte == extrinsic_format::BARE_V5
}

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

	/// Extract relay chain slot info from the parent block's setValidationData extrinsic.
	///
	/// Returns the relay chain slot that will be used in the new block's inherent,
	/// accounting for the standard slot increment.
	///
	/// This is useful for calculating the correct para slot for the new block's header,
	/// since parachain slots must be derived from the relay chain slot.
	///
	/// # Arguments
	///
	/// * `parent` - The parent block to extract relay slot from
	/// * `pallet_index` - The ParachainSystem pallet index (typically 1)
	/// * `call_index` - The setValidationData call index (typically 0)
	/// * `relay_slot_increment` - How much to increment the relay slot (typically 2)
	/// * `known_relay_block` - The relay block number from storage (used to find offset)
	///
	/// # Returns
	///
	/// The new relay chain slot if found, or None if the extrinsic couldn't be parsed.
	pub fn get_new_relay_slot(
		parent: &Block,
		pallet_index: u8,
		call_index: u8,
		relay_slot_increment: u64,
		known_relay_block: u32,
	) -> Option<u64> {
		let extrinsics = parent.extrinsics.clone();
		let ext = Self::find_validation_data_extrinsic(&extrinsics, pallet_index, call_index)?;

		// Decode compact length prefix
		let (_compact_len, body) = decode_compact_len(ext)?;

		// Find the offset of the known relay block number in the extrinsic body
		let relay_parent_offset = Self::find_u32_offset_in_body(body, known_relay_block)?;

		// Storage root is 4 bytes after relay_parent_number
		let storage_root_offset = relay_parent_offset + 4;
		if storage_root_offset + 32 > body.len() {
			return None;
		}

		let storage_root: [u8; 32] =
			body[storage_root_offset..storage_root_offset + 32].try_into().ok()?;

		// max_pov_size is 32 bytes after storage root offset
		let max_pov_offset = storage_root_offset + 32;
		if max_pov_offset + 4 > body.len() {
			return None;
		}

		// relay_chain_state starts after max_pov_size
		let proof_start = max_pov_offset + 4;
		if proof_start >= body.len() {
			return None;
		}

		// Parse the relay_chain_state (Vec<Vec<u8>>)
		let (proof_nodes, _) = parse_vec_vec_u8(&body[proof_start..])?;

		eprintln!("[get_new_relay_slot] Found {} proof nodes", proof_nodes.len());
		eprintln!("[get_new_relay_slot] Storage root: 0x{}", hex::encode(&storage_root));

		// Decode proof to get slot
		let entries = crate::proof::decode_proof(&storage_root, &proof_nodes).ok()?;
		let slot_bytes = entries.get(well_known_keys::CURRENT_SLOT)?;

		if slot_bytes.len() >= 8 {
			let current_slot = u64::from_le_bytes(slot_bytes[..8].try_into().ok()?);
			eprintln!(
				"[get_new_relay_slot] CURRENT_SLOT from proof: {} -> {} (increment: {})",
				current_slot, current_slot.saturating_add(relay_slot_increment), relay_slot_increment
			);
			Some(current_slot.saturating_add(relay_slot_increment))
		} else {
			None
		}
	}

	/// Extract relay chain slot info asynchronously from the parent block.
	///
	/// This reads the relay block number from storage and uses it to parse
	/// the setValidationData extrinsic.
	pub async fn get_new_relay_slot_async(
		parent: &Block,
		pallet_index: u8,
		call_index: u8,
		relay_slot_increment: u64,
	) -> Option<u64> {
		// Get the known relay block number from storage
		let known_relay_block = Self::read_last_relay_chain_block_number(parent).await?;
		Self::get_new_relay_slot(
			parent,
			pallet_index,
			call_index,
			relay_slot_increment,
			known_relay_block,
		)
	}

	/// Calculate the para slot from a relay chain slot.
	///
	/// For parachains, the relationship is:
	/// `para_slot = relay_slot * RELAY_SLOT_DURATION_MS / PARA_SLOT_DURATION_MS`
	///
	/// For most Aura parachains with 12s blocks and 6s relay blocks:
	/// `para_slot = relay_slot / 2`
	///
	/// # Arguments
	///
	/// * `relay_slot` - The relay chain slot
	/// * `relay_slot_duration_ms` - Relay chain slot duration in ms (typically 6000)
	/// * `para_slot_duration_ms` - Parachain slot duration in ms (typically 12000)
	pub fn relay_slot_to_para_slot(
		relay_slot: u64,
		relay_slot_duration_ms: u64,
		para_slot_duration_ms: u64,
	) -> u64 {
		let relay_timestamp = relay_slot.saturating_mul(relay_slot_duration_ms);
		relay_timestamp / para_slot_duration_ms
	}

	/// Compute the storage key for `ParachainSystem::LastRelayChainBlockNumber`.
	fn last_relay_chain_block_number_key() -> Vec<u8> {
		let pallet_hash = sp_core::twox_128(b"ParachainSystem");
		let storage_hash = sp_core::twox_128(b"LastRelayChainBlockNumber");
		[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
	}

	/// Compute the storage key for `ParachainInfo::ParachainId`.
	fn parachain_id_key() -> Vec<u8> {
		let pallet_hash = sp_core::twox_128(b"ParachainInfo");
		let storage_hash = sp_core::twox_128(b"ParachainId");
		[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
	}

	/// Compute the storage key for `AuraExt::SlotInfo`.
	///
	/// This storage tracks the relay chain slot used for the last authored block,
	/// used by `FixedVelocityConsensusHook` to validate slot progression.
	fn aura_ext_slot_info_key() -> Vec<u8> {
		let pallet_hash = sp_core::twox_128(b"AuraExt");
		let storage_hash = sp_core::twox_128(b"SlotInfo");
		[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
	}

	/// Read the AuraExt SlotInfo from storage.
	///
	/// Returns (relay_chain_slot, authored_count) if present.
	/// This is the slot that was used for the last authored parachain block.
	async fn read_aura_ext_slot_info(parent: &Block) -> Option<(u64, u32)> {
		let key = Self::aura_ext_slot_info_key();
		let storage = parent.storage();

		match storage.get(parent.number, &key).await {
			Ok(Some(entry)) if entry.value.is_some() => {
				let bytes = entry.value.as_ref()?;
				// SlotInfo is encoded as: slot (u64) + authored_count (u32) = 12 bytes
				if bytes.len() >= 12 {
					let slot = u64::from_le_bytes(bytes[..8].try_into().ok()?);
					let authored_count = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
					Some((slot, authored_count))
				} else {
					eprintln!(
						"[ParachainInherent] AuraExt::SlotInfo has unexpected size: {} bytes",
						bytes.len()
					);
					None
				}
			},
			_ => None,
		}
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

	/// Read the parachain ID from storage.
	async fn read_parachain_id(parent: &Block) -> Option<u32> {
		let key = Self::parachain_id_key();
		let storage = parent.storage();

		match storage.get(parent.number, &key).await {
			Ok(Some(entry)) if entry.value.is_some() => {
				let bytes = entry.value.as_ref()?;
				u32::decode(&mut &bytes[..]).ok()
			},
			_ => None,
		}
	}

	/// Encode the parent block header as `HeadData` (SCALE-encoded Vec<u8>).
	fn encode_head_data(header_bytes: &[u8]) -> Vec<u8> {
		// HeadData is just a SCALE-encoded Vec<u8>
		// The header bytes need to be wrapped in a compact length prefix
		Compact(header_bytes.len() as u32).encode().into_iter().chain(header_bytes.iter().copied()).collect()
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

			// Check version byte - must be a bare/unsigned extrinsic (V4: 0x04 or V5: 0x05)
			let version_byte = *remainder.first()?;
			if !is_bare_extrinsic(version_byte) {
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

	/// Patch the validation data extrinsic with updated relay chain state.
	///
	/// This updates:
	/// 1. The parent_head field (to match fork point header)
	/// 2. The relay_parent_number (incremented)
	/// 3. The CURRENT_SLOT in the relay chain state proof
	/// 4. The paraHead(para_id) in the proof (parent block header)
	/// 5. The relay_parent_storage_root (to match the new proof)
	fn patch_validation_data(
		extrinsic: &[u8],
		known_relay_block: u32,
		relay_parent_increment: u32,
		slot_increment: u64,
		para_id: Option<u32>,
		parent_header: Option<&[u8]>,
	) -> Option<Vec<u8>> {
		// Decode compact length prefix
		let (_compact_len, body) = decode_compact_len(extrinsic)?;

		// Find the offset of the known relay block number in the extrinsic body
		let relay_parent_offset = Self::find_u32_offset_in_body(body, known_relay_block)?;

		eprintln!(
			"[ParachainInherent] Found relay_parent_number {} at body offset {}",
			known_relay_block, relay_parent_offset
		);

		// Parse the parent_head field (comes right after version + pallet + call)
		// Structure: version(1) + pallet(1) + call(1) + parent_head(Vec<u8>)
		let parent_head_start = 3; // After version + pallet + call
		let (parent_head_len, parent_head_consumed) =
			decode_compact_len_with_consumed(&body[parent_head_start..])?;

		let parent_head_data_start = parent_head_start + parent_head_consumed;
		if parent_head_data_start + parent_head_len as usize > body.len() {
			eprintln!("[ParachainInherent] Body too short for parent_head data");
			return None;
		}

		let parent_head_in_ext = &body[parent_head_data_start..parent_head_data_start + parent_head_len as usize];
		eprintln!(
			"[ParachainInherent] parent_head in extrinsic: {} bytes, hash=0x{}",
			parent_head_in_ext.len(),
			hex::encode(&sp_core::blake2_256(parent_head_in_ext)[..8])
		);
		if let Some(header) = parent_header {
			eprintln!(
				"[ParachainInherent] parent_header we want: {} bytes, hash=0x{}",
				header.len(),
				hex::encode(&sp_core::blake2_256(header)[..8])
			);
			if parent_head_in_ext != header {
				eprintln!(
					"[ParachainInherent] NOTE: Will replace parent_head in extrinsic with fork point header"
				);
			}
		}

		// Structure after version + pallet + call:
		// - parent_head: Vec<u8> (compact length + bytes) - variable
		// - relay_parent_number: u32 (4 bytes)
		// - relay_parent_storage_root: H256 (32 bytes)
		// - max_pov_size: u32 (4 bytes)
		// - relay_chain_state: Vec<Vec<u8>> (compact length + items) - variable

		// Storage root is 4 bytes after relay_parent_number
		let storage_root_offset = relay_parent_offset + 4;
		if storage_root_offset + 32 > body.len() {
			eprintln!("[ParachainInherent] Body too short for storage root");
			return None;
		}

		let current_storage_root: [u8; 32] =
			body[storage_root_offset..storage_root_offset + 32].try_into().ok()?;

		eprintln!(
			"[ParachainInherent] Current storage root: 0x{}",
			hex::encode(&current_storage_root)
		);

		// max_pov_size is 32 bytes after storage root offset
		let max_pov_offset = storage_root_offset + 32;
		if max_pov_offset + 4 > body.len() {
			eprintln!("[ParachainInherent] Body too short for max_pov_size");
			return None;
		}

		// relay_chain_state starts after max_pov_size
		let proof_start = max_pov_offset + 4;
		if proof_start >= body.len() {
			eprintln!("[ParachainInherent] Body too short for proof");
			return None;
		}

		// Parse the relay_chain_state (Vec<Vec<u8>>)
		let proof_nodes = match parse_vec_vec_u8(&body[proof_start..]) {
			Some((nodes, _consumed)) => nodes,
			None => {
				eprintln!("[ParachainInherent] Failed to parse relay_chain_state");
				return None;
			},
		};

		eprintln!("[ParachainInherent] Found {} proof nodes", proof_nodes.len());

		// Decode the proof to get current slot
		let decoded_entries = match proof::decode_proof(&current_storage_root, &proof_nodes) {
			Ok(entries) => entries,
			Err(e) => {
				eprintln!("[ParachainInherent] Failed to decode proof: {}", e);
				// Fall back to just patching relay_parent_number
				return Self::patch_relay_parent_number(
					extrinsic,
					known_relay_block,
					relay_parent_increment,
				);
			},
		};

		// Compare paraHead in proof with parent_head in extrinsic
		if let Some(pid) = para_id {
			let para_head_key = well_known_keys::para_head(pid);
			if let Some(para_head_value) = decoded_entries.get(&para_head_key) {
				eprintln!(
					"[ParachainInherent] paraHead in ORIGINAL proof: {} bytes, hash=0x{}",
					para_head_value.len(),
					hex::encode(&sp_core::blake2_256(para_head_value)[..8])
				);
				// Check if it matches parent_header
				if let Some(header) = parent_header {
					// paraHead is HeadData (Vec<u8>), so it's SCALE-encoded header
					// The first bytes are the compact length prefix
					if para_head_value.len() >= header.len() {
						// Try to extract the raw header from HeadData
						if let Some((head_len, consumed)) = decode_compact_len_with_consumed(para_head_value) {
							let raw_header = &para_head_value[consumed..consumed + head_len as usize];
							eprintln!(
								"[ParachainInherent] paraHead raw header: {} bytes, hash=0x{}",
								raw_header.len(),
								hex::encode(&sp_core::blake2_256(raw_header)[..8])
							);
							if raw_header == header {
								eprintln!("[ParachainInherent] paraHead matches parent_header!");
							} else {
								eprintln!("[ParachainInherent] WARNING: paraHead does NOT match parent_header!");
							}
						}
					}
				}
			}
		}

		// Debug: Print all keys in the original proof
		eprintln!(
			"[ParachainInherent] Original proof contains {} entries:",
			decoded_entries.len()
		);
		for (key, value) in &decoded_entries {
			let key_name = match key.as_slice() {
				k if k == well_known_keys::CURRENT_SLOT => "CURRENT_SLOT".to_string(),
				k if k == well_known_keys::EPOCH_INDEX => "EPOCH_INDEX".to_string(),
				k if k == well_known_keys::ACTIVE_CONFIG => "ACTIVE_CONFIG".to_string(),
				k if k == well_known_keys::AUTHORITIES => "AUTHORITIES".to_string(),
				k if k == well_known_keys::CURRENT_BLOCK_RANDOMNESS => {
					"CURRENT_BLOCK_RANDOMNESS".to_string()
				},
				k if k == well_known_keys::ONE_EPOCH_AGO_RANDOMNESS => {
					"ONE_EPOCH_AGO_RANDOMNESS".to_string()
				},
				k if k == well_known_keys::TWO_EPOCHS_AGO_RANDOMNESS => {
					"TWO_EPOCHS_AGO_RANDOMNESS".to_string()
				},
				k if k.starts_with(well_known_keys::PARA_HEAD_PREFIX) => {
					"PARA_HEAD(...)".to_string()
				},
				_ => format!("0x{}", hex::encode(key)),
			};
			eprintln!("  - {} ({} bytes)", key_name, value.len());
		}

		// Get current slot from proof
		let current_slot = decoded_entries
			.get(well_known_keys::CURRENT_SLOT)
			.and_then(|bytes| {
				if bytes.len() >= 8 {
					Some(u64::from_le_bytes(bytes[..8].try_into().ok()?))
				} else {
					None
				}
			});

		eprintln!("[ParachainInherent] Current slot from proof: {:?}", current_slot);

		// Calculate new slot
		let new_slot = current_slot.map(|s| s.saturating_add(slot_increment));
		eprintln!(
			"[ParachainInherent] New slot: {:?} (increment: {})",
			new_slot, slot_increment
		);

		// Try in-place update first (preserves original proof structure)
		// Fall back to fresh rebuild if in-place fails
		let use_in_place = std::env::var("PARACHAIN_DEBUG_USE_IN_PLACE").is_ok();

		// Build updates map for in-place approach
		let mut updates: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();

		// Update CURRENT_SLOT with new value
		if let Some(slot) = new_slot {
			updates.insert(
				well_known_keys::CURRENT_SLOT.to_vec(),
				slot.to_le_bytes().to_vec(),
			);
			eprintln!(
				"[ParachainInherent] Will update CURRENT_SLOT: {:?} -> {}",
				current_slot, slot
			);
		}

		// Update paraHead(para_id) with the parent block header
		let skip_para_head = std::env::var("PARACHAIN_DEBUG_SKIP_PARA_HEAD").is_ok();
		if skip_para_head {
			eprintln!(
				"[ParachainInherent] DEBUG: Skipping paraHead update (PARACHAIN_DEBUG_SKIP_PARA_HEAD set)"
			);
		} else if let (Some(pid), Some(header)) = (para_id, parent_header) {
			let para_head_key = well_known_keys::para_head(pid);
			let head_data = Self::encode_head_data(header);
			eprintln!(
				"[ParachainInherent] Will update paraHead for para_id={} ({} bytes header -> {} bytes HeadData)",
				pid,
				header.len(),
				head_data.len()
			);
			updates.insert(para_head_key, head_data);
		}

		let (new_storage_root, new_proof_nodes) = if use_in_place {
			// Try in-place update (preserves original proof structure exactly)
			eprintln!(
				"[ParachainInherent] Using in-place proof update with {} updates on {} proof nodes",
				updates.len(),
				proof_nodes.len()
			);
			match proof::update_proof_in_place(&proof_nodes, &current_storage_root, updates.clone()) {
				Ok(result) => result,
				Err(e) => {
					eprintln!("[ParachainInherent] In-place update failed: {}, trying fresh rebuild", e);
					// Fall through to fresh rebuild
					let mut fresh_entries: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
					for (key, value) in &decoded_entries {
						fresh_entries.insert(key.clone(), Some(value.clone()));
					}
					for (key, value) in updates {
						fresh_entries.insert(key, Some(value));
					}
					match proof::create_proof_native(fresh_entries) {
						Ok(result) => result,
						Err(e) => {
							eprintln!("[ParachainInherent] Failed to create native proof: {}", e);
							return Self::patch_relay_parent_number(
								extrinsic,
								known_relay_block,
								relay_parent_increment,
							);
						},
					}
				},
			}
		} else {
			// Build a fresh proof from scratch (default)
			let mut fresh_entries: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
			for (key, value) in &decoded_entries {
				fresh_entries.insert(key.clone(), Some(value.clone()));
			}
			for (key, value) in updates {
				fresh_entries.insert(key, Some(value));
			}

			eprintln!(
				"[ParachainInherent] Building fresh proof with {} entries (original had {} decoded entries, {} proof nodes)",
				fresh_entries.len(),
				decoded_entries.len(),
				proof_nodes.len()
			);

			match proof::create_proof_native(fresh_entries) {
				Ok(result) => result,
				Err(e) => {
					eprintln!("[ParachainInherent] Failed to create native proof: {}", e);
					return Self::patch_relay_parent_number(
						extrinsic,
						known_relay_block,
						relay_parent_increment,
					);
				},
			}
		};

		// Debug: Verify the new proof can be decoded and contains all entries
		match proof::decode_proof(&new_storage_root, &new_proof_nodes) {
			Ok(new_entries) => {
				eprintln!(
					"[ParachainInherent] New proof contains {} entries (original had {})",
					new_entries.len(),
					decoded_entries.len()
				);
				// Check for missing entries from original
				for (key, _value) in &decoded_entries {
					if !new_entries.contains_key(key) {
						let key_name = match key.as_slice() {
							k if k == well_known_keys::CURRENT_SLOT => "CURRENT_SLOT".to_string(),
							_ => format!("0x{}", hex::encode(key)),
						};
						eprintln!("[ParachainInherent] WARNING: Key {} missing from new proof!", key_name);
					}
				}
				// Verify slot value in new proof
				if let Some(slot_bytes) = new_entries.get(well_known_keys::CURRENT_SLOT) {
					if slot_bytes.len() >= 8 {
						let slot_value = u64::from_le_bytes(slot_bytes[..8].try_into().unwrap());
						eprintln!(
							"[ParachainInherent] CURRENT_SLOT in new proof: {} (bytes: 0x{})",
							slot_value,
							hex::encode(slot_bytes)
						);
					}
				}
				// Verify paraHead in new proof
				if let Some(pid) = para_id {
					let para_head_key = well_known_keys::para_head(pid);
					if let Some(head_bytes) = new_entries.get(&para_head_key) {
						eprintln!(
							"[ParachainInherent] paraHead in new proof: {} bytes",
							head_bytes.len()
						);
					} else {
						eprintln!("[ParachainInherent] WARNING: paraHead missing from new proof!");
					}
				}
			},
			Err(e) => {
				eprintln!("[ParachainInherent] WARNING: Failed to decode new proof: {}", e);
			},
		}

		eprintln!(
			"[ParachainInherent] New storage root: 0x{}",
			hex::encode(&new_storage_root)
		);

		// Encode new proof as Vec<Vec<u8>>
		let new_proof_encoded = encode_vec_vec_u8(&new_proof_nodes);

		// Build the new extrinsic body with updated parent_head
		// Structure: version(1) + pallet(1) + call(1) + parent_head(Vec<u8>) + relay_parent_number(4) + storage_root(32) + max_pov_size(4) + proof(Vec<Vec<u8>>) + ...
		let mut new_body = Vec::with_capacity(body.len());

		// Copy version + pallet + call (first 3 bytes)
		new_body.extend_from_slice(&body[..3]);

		// Write new parent_head (use fork point header if available)
		let new_parent_head = parent_header.unwrap_or(parent_head_in_ext);
		new_body.extend(encode_compact_len(new_parent_head.len() as u32));
		new_body.extend_from_slice(new_parent_head);

		eprintln!(
			"[ParachainInherent] New parent_head in extrinsic: {} bytes, hash=0x{}",
			new_parent_head.len(),
			hex::encode(&sp_core::blake2_256(new_parent_head)[..8])
		);

		// Write new relay_parent_number
		// Must increment by at least 1 to pass CheckAssociatedRelayNumber which requires:
		// relay_parent_number > LastRelayChainBlockNumber
		// Since our proof is synthetic (not from a real relay block), we can set this freely.
		let new_relay_number = known_relay_block.saturating_add(relay_parent_increment);
		new_body.extend_from_slice(&new_relay_number.to_le_bytes());

		// Write new storage root
		new_body.extend_from_slice(&new_storage_root);

		// Copy max_pov_size from original (4 bytes after storage_root)
		// The original max_pov_size is at: relay_parent_offset + 4 (relay_num) + 32 (storage_root)
		new_body.extend_from_slice(&body[max_pov_offset..max_pov_offset + 4]);

		// Write new proof
		new_body.extend(new_proof_encoded);

		// Copy everything after the original proof (e.g., downward_messages, horizontal_messages)
		let (_, original_proof_consumed) = parse_vec_vec_u8(&body[proof_start..])?;
		let after_proof_offset = proof_start + original_proof_consumed;
		if after_proof_offset < body.len() {
			new_body.extend_from_slice(&body[after_proof_offset..]);
		}

		let new_slot_val = new_slot.unwrap_or_else(|| current_slot.unwrap_or(0));
		eprintln!(
			"[ParachainInherent] Patched: parent_head={} bytes, relay_parent_number: {} -> {}, slot: {:?} -> {}",
			new_parent_head.len(), known_relay_block, new_relay_number, current_slot, new_slot_val
		);

		// Re-encode with compact length prefix
		let new_compact_len = new_body.len() as u32;
		let mut result = encode_compact_len(new_compact_len);
		result.extend(new_body);

		// Verify the length changed appropriately
		if result.len() != extrinsic.len() {
			let diff = result.len() as i64 - extrinsic.len() as i64;
			eprintln!(
				"[ParachainInherent] Extrinsic size changed by {} bytes ({} -> {})",
				diff,
				extrinsic.len(),
				result.len()
			);
		}

		Some(result)
	}
}

/// Parse a SCALE-encoded Vec<Vec<u8>> from bytes.
/// Returns (parsed_vec, bytes_consumed).
fn parse_vec_vec_u8(data: &[u8]) -> Option<(Vec<Vec<u8>>, usize)> {
	let mut offset = 0;

	// Decode outer Vec length
	let (outer_len, outer_consumed) = decode_compact_len_with_consumed(&data[offset..])?;
	offset += outer_consumed;

	let mut result = Vec::with_capacity(outer_len as usize);

	for _ in 0..outer_len {
		// Decode inner Vec length
		let (inner_len, inner_consumed) = decode_compact_len_with_consumed(&data[offset..])?;
		offset += inner_consumed;

		// Extract inner bytes
		if offset + inner_len as usize > data.len() {
			return None;
		}
		result.push(data[offset..offset + inner_len as usize].to_vec());
		offset += inner_len as usize;
	}

	Some((result, offset))
}

/// Encode a Vec<Vec<u8>> as SCALE.
fn encode_vec_vec_u8(data: &[Vec<u8>]) -> Vec<u8> {
	let mut result = encode_compact_len(data.len() as u32);
	for inner in data {
		result.extend(encode_compact_len(inner.len() as u32));
		result.extend(inner);
	}
	result
}

/// Decode compact length and return (value, bytes_consumed).
fn decode_compact_len_with_consumed(data: &[u8]) -> Option<(u32, usize)> {
	if data.is_empty() {
		return None;
	}

	let first_byte = data[0];
	let mode = first_byte & compact_mode::MODE_MASK;

	match mode {
		compact_mode::SINGLE_BYTE => {
			let len = (first_byte >> compact_mode::VALUE_SHIFT) as u32;
			Some((len, 1))
		},
		compact_mode::TWO_BYTE => {
			if data.len() < 2 {
				return None;
			}
			let len = (u16::from_le_bytes([data[0], data[1]]) >> compact_mode::VALUE_SHIFT) as u32;
			Some((len, 2))
		},
		compact_mode::FOUR_BYTE => {
			if data.len() < 4 {
				return None;
			}
			let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]])
				>> compact_mode::VALUE_SHIFT;
			Some((len, 4))
		},
		compact_mode::BIG_INTEGER => {
			let bytes_following = ((first_byte >> compact_mode::VALUE_SHIFT) + 4) as usize;
			if data.len() < 1 + bytes_following {
				return None;
			}
			// Only support values that fit in u32 (max 4 bytes)
			if bytes_following > 4 {
				return None;
			}
			let mut value: u32 = 0;
			for (i, &byte) in data[1..1 + bytes_following].iter().enumerate() {
				value |= (byte as u32) << (i * 8);
			}
			Some((value, 1 + bytes_following))
		},
		_ => None,
	}
}

/// Decode a compact length prefix from SCALE-encoded data.
/// Returns (length_value, remaining_bytes).
fn decode_compact_len(data: &[u8]) -> Option<(u32, &[u8])> {
	if data.is_empty() {
		return None;
	}

	let first_byte = data[0];
	let mode = first_byte & compact_mode::MODE_MASK;

	match mode {
		compact_mode::SINGLE_BYTE => {
			let len = (first_byte >> compact_mode::VALUE_SHIFT) as u32;
			Some((len, &data[1..]))
		},
		compact_mode::TWO_BYTE => {
			if data.len() < 2 {
				return None;
			}
			let len = (u16::from_le_bytes([data[0], data[1]]) >> compact_mode::VALUE_SHIFT) as u32;
			Some((len, &data[2..]))
		},
		compact_mode::FOUR_BYTE => {
			if data.len() < 4 {
				return None;
			}
			let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]])
				>> compact_mode::VALUE_SHIFT;
			Some((len, &data[4..]))
		},
		compact_mode::BIG_INTEGER => {
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

		// Debug: dump first few bytes of each extrinsic
		for (i, ext) in parent.extrinsics.iter().enumerate() {
			let preview = ext.iter().take(20).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join("");
			eprintln!(
				"[ParachainInherent] Extrinsic {i} ({} bytes): 0x{preview}...",
				ext.len()
			);
			// Try to decode compact length and show what's inside
			if let Some((len, body)) = decode_compact_len(ext) {
				let version = body.first().copied().unwrap_or(0);
				let pallet = body.get(1).copied().unwrap_or(0);
				let call = body.get(2).copied().unwrap_or(0);
				eprintln!(
					"[ParachainInherent]   -> compact_len={}, version=0x{:02x}, pallet={}, call={}",
					len, version, pallet, call
				);
			}
		}

		// Read the last relay chain block number from storage
		let last_relay_block = Self::read_last_relay_chain_block_number(parent).await;
		eprintln!(
			"[ParachainInherent] lastRelayChainBlockNumber from storage: {:?}",
			last_relay_block
		);

		// Read the parachain ID from storage (needed for paraHead key)
		let para_id = Self::read_parachain_id(parent).await;
		eprintln!("[ParachainInherent] ParachainId from storage: {:?}", para_id);

		// Read the AuraExt::SlotInfo from storage - this is what the consensus hook checks
		let slot_info = Self::read_aura_ext_slot_info(parent).await;
		eprintln!(
			"[ParachainInherent] AuraExt::SlotInfo from storage: {:?}",
			slot_info
		);
		if let Some((relay_slot, authored_count)) = slot_info {
			eprintln!(
				"[ParachainInherent]   -> relay_chain_slot: {}, authored_count: {}",
				relay_slot, authored_count
			);
		}

		// Get the parent block header (needed for paraHead value)
		let parent_header = &parent.header;
		eprintln!(
			"[ParachainInherent] Parent header: {} bytes",
			parent_header.len()
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

				// DEBUG: Various debugging modes
				// NO_PATCH: Use extrinsic unchanged
				// NO_PROOF_REBUILD: Only patch relay_parent_number, keep original proof
				if std::env::var("PARACHAIN_DEBUG_NO_PATCH").is_ok() {
					eprintln!(
						"[ParachainInherent] DEBUG: Using extrinsic unchanged (no patching)"
					);
					return Ok(vec![ext.clone()]);
				}
				if std::env::var("PARACHAIN_DEBUG_NO_PROOF_REBUILD").is_ok() {
					eprintln!(
						"[ParachainInherent] DEBUG: Only patching relay_parent_number (no proof rebuild)"
					);
					// Use same increment values calculated below
					if let Some(patched) = Self::patch_relay_parent_number(ext, relay_block, 2) {
						return Ok(vec![patched]);
					}
				}

				// Calculate slot increment based on relay chain slot duration (6s) vs parachain
				// For 12s parachain blocks, relay chain advances by 2 slots
				// For 6s parachain blocks, relay chain advances by 1 slot
				// Default to 2 (12s parachain blocks / 6s relay slots)
				let relay_slot_increment: u64 = 2;
				let relay_parent_increment: u32 = relay_slot_increment as u32;

				// Try to patch with full proof manipulation (relay_parent_number + slot + paraHead + root)
				match Self::patch_validation_data(
					ext,
					relay_block,
					relay_parent_increment,
					relay_slot_increment,
					para_id,
					Some(parent_header),
				) {
					Some(patched) => {
						eprintln!(
							"[ParachainInherent] Patched extrinsic with proof update ({} bytes)",
							patched.len()
						);
						Ok(vec![patched])
					},
					None => {
						eprintln!(
							"[ParachainInherent] Failed to patch validation data, trying simple patch"
						);
						// Fall back to simple relay_parent_number patch
						match Self::patch_relay_parent_number(ext, relay_block, relay_parent_increment)
						{
							Some(patched) => {
								eprintln!(
									"[ParachainInherent] Patched extrinsic (simple) ({} bytes)",
									patched.len()
								);
								Ok(vec![patched])
							},
							None => {
								eprintln!(
									"[ParachainInherent] Failed to find relay block {} in extrinsic",
									relay_block
								);
								// Can't patch - return empty and let block building fail with clear
								// error
								Ok(vec![])
							},
						}
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
	fn find_validation_data_extrinsic_finds_matching_v4() {
		// Create a mock V4 extrinsic: compact_len + version + pallet + call + data
		let mut ext = encode_compact_len(10); // length prefix
		ext.push(extrinsic_format::BARE_V4); // V4 version byte
		ext.push(51); // pallet index
		ext.push(0); // call index
		ext.extend([0u8; 7]); // padding to reach length

		let extrinsics = vec![ext.clone()];
		let result = ParachainInherent::find_validation_data_extrinsic(&extrinsics, 51, 0);
		assert!(result.is_some());
	}

	#[test]
	fn find_validation_data_extrinsic_finds_matching_v5() {
		// Create a mock V5 extrinsic: compact_len + version + pallet + call + data
		let mut ext = encode_compact_len(10); // length prefix
		ext.push(extrinsic_format::BARE_V5); // V5 bare version byte (0x05)
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
		ext.push(extrinsic_format::BARE_V4);
		ext.push(10); // different pallet
		ext.push(5); // different call
		ext.extend([0u8; 2]);

		let extrinsics = vec![ext];
		let result = ParachainInherent::find_validation_data_extrinsic(&extrinsics, 51, 0);
		assert!(result.is_none());
	}

	#[test]
	fn find_validation_data_extrinsic_ignores_signed_extrinsic() {
		// Create a mock signed V5 extrinsic (0x85 = signed V5)
		let mut ext = encode_compact_len(10);
		ext.push(0x85); // signed V5
		ext.push(51); // pallet index
		ext.push(0); // call index
		ext.extend([0u8; 7]);

		let extrinsics = vec![ext];
		let result = ParachainInherent::find_validation_data_extrinsic(&extrinsics, 51, 0);
		assert!(result.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn apply_parachain_inherent_integration() {
		use crate::{
			Block, BlockBuilder, ExecutorConfig, ForkRpcClient, RuntimeExecutor,
			SignatureMockMode, StorageCache, builder::create_next_header_with_specific_slot,
			inherent::{ParachainInherent, TimestampInherent},
		};

		// Connect to a live parachain (Collectives on Paseo)
		let endpoint: url::Url = "wss://collectives-paseo.rpc.amforc.com"
			.parse()
			.expect("Invalid endpoint URL");
		let rpc = ForkRpcClient::connect(&endpoint).await.expect("Failed to connect to parachain");

		let block_hash = rpc.finalized_head().await.expect("Failed to get finalized head");
		let runtime_code =
			rpc.runtime_code(block_hash).await.expect("Failed to fetch runtime code");

		let cache = StorageCache::in_memory().await.expect("Failed to create cache");
		let block = Block::fork_point(&endpoint, cache, block_hash.into())
			.await
			.expect("Failed to create fork point");

		// Use executor config with allow_unresolved_imports for inherents
		// (as recommended by Chopsticks for handling inherent extrinsics)
		let config = ExecutorConfig {
			allow_unresolved_imports: true,
			max_log_level: 5, // Trace level to capture all runtime logs
			signature_mock: SignatureMockMode::None,
			..Default::default()
		};
		let executor = RuntimeExecutor::with_config(runtime_code, None, config)
			.expect("Failed to create executor");

		// For parachains, the para slot must be derived from the relay chain slot.
		// The ConsensusHook checks: para_slot == relay_slot * RELAY_DURATION / PARA_DURATION
		// For Collectives-Paseo (6s relay slots, 12s para slots): para_slot = relay_slot / 2
		const RELAY_SLOT_INCREMENT: u64 = 2;
		const RELAY_SLOT_DURATION_MS: u64 = 6000;
		const PARA_SLOT_DURATION_MS: u64 = 12000;

		// Get the relay slot that will be used in the new block's inherent
		let new_relay_slot = ParachainInherent::get_new_relay_slot_async(
			&block,
			1, // ParachainSystem pallet index
			0, // setValidationData call index
			RELAY_SLOT_INCREMENT,
		)
		.await
		.expect("Failed to get relay slot from parent block");

		eprintln!("[Test] New relay slot will be: {}", new_relay_slot);

		// Calculate the para slot from the relay slot
		let para_slot = ParachainInherent::relay_slot_to_para_slot(
			new_relay_slot,
			RELAY_SLOT_DURATION_MS,
			PARA_SLOT_DURATION_MS,
		);
		eprintln!("[Test] Calculated para slot: {}", para_slot);

		// Create header with the calculated para slot
		let header = create_next_header_with_specific_slot(&block, para_slot);

		// ParachainInherent must come before Timestamp for parachains
		let providers: Vec<Box<dyn crate::InherentProvider>> = vec![
			Box::new(ParachainInherent::new()),
			Box::new(TimestampInherent::default_relay()),
		];

		let mut builder = BlockBuilder::new(block, executor, header, providers);

		builder.initialize().await.expect("initialize should succeed");

		// Apply inherents - the ConsensusHook should now pass because
		// para_slot matches the expected value derived from the relay slot
		let result = builder.apply_inherents().await;

		assert!(
			result.is_ok(),
			"Expected apply_inherents() to succeed. Error: {:?}",
			result.err()
		);
	}
}
