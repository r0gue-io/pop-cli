// SPDX-License-Identifier: GPL-3.0

//! Merkle trie proof manipulation utilities.
//!
//! This module provides functions for decoding and creating merkle proofs
//! for relay chain state verification in parachains.
//!
//! # Overview
//!
//! When building blocks for parachains, the `setValidationData` inherent
//! includes a merkle proof of the relay chain state. This proof must be
//! updated to reflect the new relay chain slot and state root.
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::proof::{decode_proof, create_proof};
//!
//! // Decode existing proof
//! let entries = decode_proof(&root_hash, &proof_nodes)?;
//!
//! // Update CURRENT_SLOT
//! let updates = vec![(CURRENT_SLOT_KEY.to_vec(), Some(new_slot_bytes))];
//!
//! // Create new proof with updated root
//! let (new_root, new_nodes) = create_proof(&proof_nodes, updates)?;
//! ```

use smoldot::trie::{
	bytes_to_nibbles, nibbles_to_bytes_suffix_extend,
	proof_decode::{self, StorageValue},
	proof_encode::ProofBuilder,
	trie_node, trie_structure, Nibble,
};
use std::collections::BTreeMap;

/// Well-known relay chain storage keys.
///
/// These keys are required by parachain runtimes when validating the relay chain state proof.
/// See: https://github.com/AcalaNetwork/chopsticks/blob/master/packages/core/src/utils/proof.ts
pub mod well_known_keys {
	/// Current slot key: `Babe::CurrentSlot` storage.
	/// `twox_128("Babe") ++ twox_128("CurrentSlot")`
	pub const CURRENT_SLOT: &[u8] = &[
		0x1c, 0xb6, 0xf3, 0x6e, 0x02, 0x7a, 0xbb, 0x20, 0x91, 0xcf, 0xb5, 0x11, 0x0a, 0xb5, 0x08,
		0x7f, 0x06, 0x15, 0x5b, 0x3c, 0xd9, 0xa8, 0xc9, 0xe5, 0xe9, 0xa2, 0x3f, 0xd5, 0xdc, 0x13,
		0xa5, 0xed,
	];

	/// Epoch index key: `Babe::EpochIndex` storage.
	pub const EPOCH_INDEX: &[u8] = &[
		0x1c, 0xb6, 0xf3, 0x6e, 0x02, 0x7a, 0xbb, 0x20, 0x91, 0xcf, 0xb5, 0x11, 0x0a, 0xb5, 0x08,
		0x7f, 0x38, 0x31, 0x6c, 0xbf, 0x8f, 0xa0, 0xda, 0x82, 0x2a, 0x20, 0xac, 0x1c, 0x55, 0xbf,
		0x1b, 0xe3,
	];

	/// Current block randomness key: `Babe::Randomness` storage.
	pub const CURRENT_BLOCK_RANDOMNESS: &[u8] = &[
		0x1c, 0xb6, 0xf3, 0x6e, 0x02, 0x7a, 0xbb, 0x20, 0x91, 0xcf, 0xb5, 0x11, 0x0a, 0xb5, 0x08,
		0x7f, 0xd0, 0x77, 0xdf, 0xdb, 0x8a, 0xdb, 0x10, 0xf7, 0x8f, 0x10, 0xa5, 0xdf, 0x87, 0x42,
		0xc5, 0x45,
	];

	/// One epoch ago randomness key.
	pub const ONE_EPOCH_AGO_RANDOMNESS: &[u8] = &[
		0x1c, 0xb6, 0xf3, 0x6e, 0x02, 0x7a, 0xbb, 0x20, 0x91, 0xcf, 0xb5, 0x11, 0x0a, 0xb5, 0x08,
		0x7f, 0x7c, 0xe6, 0x78, 0x79, 0x9d, 0x3e, 0xff, 0x02, 0x42, 0x53, 0xb9, 0x0e, 0x84, 0x92,
		0x7c, 0xc6,
	];

	/// Two epochs ago randomness key.
	pub const TWO_EPOCHS_AGO_RANDOMNESS: &[u8] = &[
		0x1c, 0xb6, 0xf3, 0x6e, 0x02, 0x7a, 0xbb, 0x20, 0x91, 0xcf, 0xb5, 0x11, 0x0a, 0xb5, 0x08,
		0x7f, 0x7a, 0x41, 0x4c, 0xb0, 0x08, 0xe0, 0xe6, 0x1e, 0x46, 0x72, 0x2a, 0xa6, 0x0a, 0xbd,
		0xd6, 0x72,
	];

	/// Active configuration key: `Configuration::ActiveConfig` storage.
	pub const ACTIVE_CONFIG: &[u8] = &[
		0x06, 0xde, 0x3d, 0x8a, 0x54, 0xd2, 0x7e, 0x44, 0xa9, 0xd5, 0xce, 0x18, 0x96, 0x18, 0xf2,
		0x2d, 0xb4, 0xb4, 0x9d, 0x95, 0x32, 0x0d, 0x90, 0x21, 0x99, 0x4c, 0x85, 0x0f, 0x25, 0xb8,
		0xe3, 0x85,
	];

	/// Authorities key: `Babe::Authorities` storage.
	pub const AUTHORITIES: &[u8] = &[
		0x1c, 0xb6, 0xf3, 0x6e, 0x02, 0x7a, 0xbb, 0x20, 0x91, 0xcf, 0xb5, 0x11, 0x0a, 0xb5, 0x08,
		0x7f, 0x5e, 0x06, 0x21, 0xc4, 0x86, 0x9a, 0xa6, 0x0c, 0x02, 0xbe, 0x9a, 0xdc, 0xc9, 0x8a,
		0x0d, 0x1d,
	];

	/// Prefix for `Paras::Heads` storage map.
	/// `twox_128("Paras") ++ twox_128("Heads")`
	/// The full key is: prefix ++ twox_64(para_id_le) ++ para_id_le
	pub const PARA_HEAD_PREFIX: &[u8] = &[
		0xcd, 0x71, 0x0b, 0x30, 0xbd, 0x2e, 0xab, 0x03, 0x52, 0xdd, 0xcc, 0x26, 0x41, 0x7a, 0xa1,
		0x94, 0x1b, 0x3c, 0x25, 0x2f, 0xcb, 0x29, 0xd8, 0x8e, 0xff, 0x4f, 0x3d, 0xe5, 0xde, 0x44,
		0x76, 0xc3,
	];

	/// Compute the storage key for `Paras::Heads(para_id)`.
	/// Key format: twox_128("Paras") ++ twox_128("Heads") ++ twox_64(para_id_le) ++ para_id_le
	pub fn para_head(para_id: u32) -> Vec<u8> {
		let para_id_bytes = para_id.to_le_bytes();
		let twox_64_hash = sp_core::twox_64(&para_id_bytes);
		[PARA_HEAD_PREFIX, &twox_64_hash, &para_id_bytes].concat()
	}
}

/// Error type for proof operations.
#[derive(Debug, Clone)]
pub enum ProofError {
	/// Failed to decode the proof.
	DecodeFailed(String),
	/// Failed to encode the proof.
	EncodeFailed(String),
	/// Key not found in proof.
	KeyNotFound(Vec<u8>),
}

impl std::fmt::Display for ProofError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::DecodeFailed(msg) => write!(f, "Failed to decode proof: {}", msg),
			Self::EncodeFailed(msg) => write!(f, "Failed to encode proof: {}", msg),
			Self::KeyNotFound(key) => write!(f, "Key not found in proof: 0x{}", hex::encode(key)),
		}
	}
}

impl std::error::Error for ProofError {}

/// Encode proof nodes into the SCALE format expected by smoldot.
fn encode_proofs(nodes: &[Vec<u8>]) -> Vec<u8> {
	let mut proof = encode_scale_compact_usize(nodes.len());
	for node in nodes {
		proof.extend(encode_scale_compact_usize(node.len()));
		proof.extend(node);
	}
	proof
}

/// Encode a usize as SCALE compact format.
fn encode_scale_compact_usize(mut value: usize) -> Vec<u8> {
	let mut array = Vec::with_capacity(5);

	if value < 64 {
		array.push((value << 2) as u8);
	} else if value < (1 << 14) {
		array.push(((value & 0b111111) << 2) as u8 | 0b01);
		array.push(((value >> 6) & 0xff) as u8);
	} else if value < (1 << 30) {
		array.push(((value & 0b111111) << 2) as u8 | 0b10);
		array.push(((value >> 6) & 0xff) as u8);
		array.push(((value >> 14) & 0xff) as u8);
		array.push(((value >> 22) & 0xff) as u8);
	} else {
		let mut temp = Vec::new();
		while value != 0 {
			temp.push((value & 0xff) as u8);
			value >>= 8;
		}
		array.push(((temp.len() - 4) << 2) as u8 | 0b11);
		array.extend(temp);
	}

	array
}

/// Decode a merkle proof and return all key-value pairs.
///
/// # Arguments
///
/// * `trie_root_hash` - The expected trie root hash (32 bytes)
/// * `nodes` - The proof nodes (each node is a trie node value)
///
/// # Returns
///
/// A map of storage keys to their values.
pub fn decode_proof(
	trie_root_hash: &[u8; 32],
	nodes: &[Vec<u8>],
) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, ProofError> {
	let encoded_proof = encode_proofs(nodes);

	let config = proof_decode::Config::<Vec<u8>> { proof: encoded_proof };
	let decoded = proof_decode::decode_and_verify_proof(config)
		.map_err(|e| ProofError::DecodeFailed(e.to_string()))?;

	let mut entries = BTreeMap::new();

	for (key, entry) in decoded.iter_ordered() {
		// Only include entries from the expected trie root
		if *key.trie_root_hash != *trie_root_hash {
			continue;
		}

		// Only include entries with known storage values
		if let StorageValue::Known { value, .. } = entry.trie_node_info.storage_value {
			let key_bytes: Vec<u8> = nibbles_to_bytes_suffix_extend(key.key).collect();
			entries.insert(key_bytes, value.to_vec());
		}
	}

	Ok(entries)
}

/// Create a new merkle proof with updated values.
///
/// This follows the Chopsticks pattern:
/// 1. First apply updates to a fresh trie
/// 2. Then copy non-conflicting entries from the decoded proof (extracting from raw nodes)
/// 3. Build the proof nodes from the trie
///
/// # Arguments
///
/// * `nodes` - The original proof nodes
/// * `updates` - Key-value pairs to update. `None` value means delete the key.
///
/// # Returns
///
/// A tuple of (new_root_hash, new_proof_nodes).
pub fn create_proof(
	nodes: &[Vec<u8>],
	updates: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
) -> Result<([u8; 32], Vec<Vec<u8>>), ProofError> {
	let encoded_proof = encode_proofs(nodes);

	let config = proof_decode::Config::<Vec<u8>> { proof: encoded_proof };
	let decoded = proof_decode::decode_and_verify_proof(config)
		.map_err(|e| ProofError::DecodeFailed(e.to_string()))?;

	// Step 1: Build trie with updates FIRST (like Chopsticks)
	let mut trie = trie_structure::TrieStructure::<Vec<u8>>::new();
	let mut deletes: Vec<Vec<u8>> = vec![];

	// Insert all updates into the trie
	let mut updates_added = 0;
	for (key, value) in &updates {
		if let Some(v) = value {
			let nibbles = bytes_to_nibbles(key.iter().cloned());
			match trie.node(nibbles) {
				trie_structure::Entry::Vacant(vacant) => {
					vacant.insert_storage_value().insert(v.clone(), vec![]);
					updates_added += 1;
				},
				trie_structure::Entry::Occupied(_) => {
					// Key already exists (shouldn't happen for updates)
					return Err(ProofError::EncodeFailed(format!(
						"Duplicate key in updates: 0x{}",
						hex::encode(key)
					)));
				},
			}
		} else {
			deletes.push(key.clone());
		}
	}

	eprintln!("[create_proof] Applied {} updates to trie", updates_added);

	// Step 2: Copy entries from original proof (only if key not already in trie)
	// CRITICAL: Only copy INLINE values (state version 0), skip HASHED values (state version 1)
	// This matches Chopsticks' approach - when we copy a hashed value and re-encode as unhashed,
	// the merkle structure changes because the node encoding differs:
	// - Original: StorageValue::Hashed(32-byte-hash)
	// - Our copy: StorageValue::Unhashed(full-value) which may be >32 bytes
	// Skipping hashed values preserves the proof structure for chains using state version 1.
	let mut entries_copied = 0;
	let mut entries_skipped_hashed = 0;
	let mut entries_skipped_occupied = 0;
	let mut entries_skipped_incomplete = 0;
	let mut entries_skipped_none = 0;

	for (entry_key, entry) in decoded.iter_ordered() {
		// Clone the iterator since we need to use it twice
		let key_nibbles: Vec<Nibble> = entry_key.key.collect();

		// Check if key already exists in trie (from updates)
		match trie.node(key_nibbles.iter().cloned()) {
			trie_structure::Entry::Vacant(vacant) => {
				// Use the smoldot decoder's resolved storage value
				match entry.trie_node_info.storage_value {
					StorageValue::Known { value, inline } => {
						// Only copy inline values (state version 0)
						// Skip hashed values to preserve merkle structure
						if inline {
							vacant.insert_storage_value().insert(value.to_vec(), vec![]);
							entries_copied += 1;
						} else {
							entries_skipped_hashed += 1;
						}
					},
					StorageValue::HashKnownValueMissing(_hash) => {
						// Incomplete proof - hash known but value bytes not in proof
						// We can't recover this value
						entries_skipped_incomplete += 1;
					},
					StorageValue::None => {
						// Branch node without storage value - skip
						entries_skipped_none += 1;
					},
				}
			},
			trie_structure::Entry::Occupied(_) => {
				// Key already exists from updates, skip
				entries_skipped_occupied += 1;
			},
		}
	}

	eprintln!(
		"[create_proof] Copied {} inline entries (skipped: {} hashed, {} occupied, {} incomplete, {} none)",
		entries_copied, entries_skipped_hashed,
		entries_skipped_occupied, entries_skipped_incomplete, entries_skipped_none
	);

	// Step 3: Handle deletes
	let mut deletes_applied = 0;
	for key in deletes {
		let nibbles = bytes_to_nibbles(key.iter().cloned());
		if let trie_structure::Entry::Occupied(occupied) = trie.node(nibbles) {
			if occupied.has_storage_value() {
				occupied.into_storage().unwrap().remove();
				deletes_applied += 1;
			}
		}
	}
	if deletes_applied > 0 {
		eprintln!("[create_proof] Applied {} deletes", deletes_applied);
	}

	// Step 4: Build the proof - always use Unhashed (state version 0) like Chopsticks
	let mut proof_builder = ProofBuilder::new();

	for node_index in trie.clone().iter_unordered() {
		let key: Vec<Nibble> = trie.node_full_key_by_index(node_index).unwrap().collect();

		let has_storage_value = trie.node_by_index(node_index).unwrap().has_storage_value();
		let storage_value = if has_storage_value {
			trie.node_by_index(node_index)
				.unwrap()
				.into_storage()
				.unwrap()
				.user_data()
				.clone()
		} else {
			vec![]
		};

		let decoded_node = trie_node::Decoded {
			children: std::array::from_fn(|nibble| {
				let nibble = Nibble::try_from(u8::try_from(nibble).unwrap()).unwrap();
				if trie.node_by_index(node_index).unwrap().child_user_data(nibble).is_some() {
					Some(&[][..])
				} else {
					None
				}
			}),
			partial_key: trie
				.node_by_index(node_index)
				.unwrap()
				.partial_key()
				.collect::<Vec<_>>()
				.into_iter(),
			// Always use Unhashed like Chopsticks does
			storage_value: if has_storage_value {
				trie_node::StorageValue::Unhashed(&storage_value[..])
			} else {
				trie_node::StorageValue::None
			},
		};

		let node_value = trie_node::encode_to_vec(decoded_node)
			.map_err(|e| ProofError::EncodeFailed(format!("Failed to encode node: {:?}", e)))?;

		proof_builder.set_node_value(&key, &node_value, None);
	}

	// Step 5: Finalize the proof
	if proof_builder.missing_node_values().next().is_some() {
		return Err(ProofError::EncodeFailed("Proof has missing node values".to_string()));
	}

	proof_builder.make_coherent();
	let trie_root_hash = proof_builder
		.trie_root_hash()
		.ok_or_else(|| ProofError::EncodeFailed("Failed to compute trie root hash".to_string()))?;

	// Extract nodes from the builder
	// The build() iterator returns: [count, len1, node1, len2, node2, ...]
	// We skip the count, then take every odd-indexed item (the actual nodes)
	let new_nodes: Vec<Vec<u8>> = proof_builder
		.build()
		.skip(1) // Skip the node count
		.enumerate()
		.filter(|(i, _)| i % 2 != 0) // Keep only odd indices (nodes, not lengths)
		.map(|(_, chunk)| chunk.as_ref().to_vec())
		.collect();

	eprintln!(
		"[create_proof] Output: {} nodes, total {} bytes",
		new_nodes.len(),
		new_nodes.iter().map(|n| n.len()).sum::<usize>()
	);

	// Debug: verify root hash is in the proof nodes
	let root_in_proof = new_nodes.iter().any(|node| {
		let hash = sp_core::blake2_256(node);
		hash == trie_root_hash
	});
	eprintln!(
		"[create_proof] Root hash 0x{} present in proof: {}",
		hex::encode(&trie_root_hash),
		root_in_proof
	);

	if !root_in_proof {
		// List hashes of all nodes for debugging
		eprintln!("[create_proof] Node hashes in proof:");
		for (i, node) in new_nodes.iter().enumerate() {
			let hash = sp_core::blake2_256(node);
			eprintln!("  Node {}: 0x{} ({} bytes)", i, hex::encode(&hash), node.len());
		}
	}

	Ok((trie_root_hash, new_nodes))
}

/// Parse proof bytes into individual node vectors.
/// Note: This function parses SCALE compact encoded proofs, which is different
/// from the format output by ProofBuilder::build(). Kept for potential debugging use.
#[allow(dead_code)]
fn parse_proof_bytes(proof: &[u8]) -> Result<Vec<Vec<u8>>, ProofError> {
	let mut nodes = Vec::new();
	let mut offset = 0;

	// First, decode the number of nodes (SCALE compact)
	let (num_nodes, consumed) = decode_scale_compact_usize(&proof[offset..])
		.ok_or_else(|| ProofError::DecodeFailed("Failed to decode node count".to_string()))?;
	offset += consumed;

	for _ in 0..num_nodes {
		// Decode node length
		let (node_len, consumed) = decode_scale_compact_usize(&proof[offset..])
			.ok_or_else(|| ProofError::DecodeFailed("Failed to decode node length".to_string()))?;
		offset += consumed;

		// Extract node bytes
		if offset + node_len > proof.len() {
			return Err(ProofError::DecodeFailed("Proof truncated".to_string()));
		}
		nodes.push(proof[offset..offset + node_len].to_vec());
		offset += node_len;
	}

	Ok(nodes)
}

/// Decode a SCALE compact usize from bytes.
/// Returns (value, bytes_consumed).
fn decode_scale_compact_usize(data: &[u8]) -> Option<(usize, usize)> {
	if data.is_empty() {
		return None;
	}

	let first_byte = data[0];
	let mode = first_byte & 0b11;

	match mode {
		0b00 => Some(((first_byte >> 2) as usize, 1)),
		0b01 => {
			if data.len() < 2 {
				return None;
			}
			let value = (u16::from_le_bytes([data[0], data[1]]) >> 2) as usize;
			Some((value, 2))
		},
		0b10 => {
			if data.len() < 4 {
				return None;
			}
			let value = (u32::from_le_bytes([data[0], data[1], data[2], data[3]]) >> 2) as usize;
			Some((value, 4))
		},
		0b11 => {
			let bytes_following = ((first_byte >> 2) + 4) as usize;
			if data.len() < 1 + bytes_following {
				return None;
			}
			let mut value: usize = 0;
			for (i, &byte) in data[1..1 + bytes_following].iter().enumerate() {
				value |= (byte as usize) << (i * 8);
			}
			Some((value, 1 + bytes_following))
		},
		_ => None,
	}
}

/// Update the CURRENT_SLOT in a relay chain state proof.
///
/// # Arguments
///
/// * `nodes` - The original proof nodes
/// * `current_slot` - The current slot value to set
///
/// # Returns
///
/// A tuple of (new_root_hash, new_proof_nodes).
pub fn update_current_slot(
	nodes: &[Vec<u8>],
	current_slot: u64,
) -> Result<([u8; 32], Vec<Vec<u8>>), ProofError> {
	let mut updates = BTreeMap::new();
	updates.insert(well_known_keys::CURRENT_SLOT.to_vec(), Some(current_slot.to_le_bytes().to_vec()));
	create_proof(nodes, updates)
}

/// Update the CURRENT_SLOT using only well-known relay chain keys.
///
/// This follows the Chopsticks pattern of creating a minimal proof with only
/// the keys that the parachain runtime actually verifies.
///
/// # Arguments
///
/// * `nodes` - The original proof nodes
/// * `trie_root` - The expected root hash for decoding the original proof
/// * `new_slot` - The new slot value to set
///
/// # Returns
///
/// A tuple of (new_root_hash, new_proof_nodes).
pub fn update_current_slot_minimal(
	nodes: &[Vec<u8>],
	trie_root: &[u8; 32],
	new_slot: u64,
) -> Result<([u8; 32], Vec<Vec<u8>>), ProofError> {
	// Decode original proof to extract well-known key values
	let entries = decode_proof(trie_root, nodes)?;

	// Build updates with only WELL_KNOWN_KEYS (like Chopsticks does)
	let well_known_keys_list: &[&[u8]] = &[
		well_known_keys::CURRENT_SLOT,
		well_known_keys::EPOCH_INDEX,
		well_known_keys::CURRENT_BLOCK_RANDOMNESS,
		well_known_keys::ONE_EPOCH_AGO_RANDOMNESS,
		well_known_keys::TWO_EPOCHS_AGO_RANDOMNESS,
		well_known_keys::ACTIVE_CONFIG,
		well_known_keys::AUTHORITIES,
	];

	let mut updates = BTreeMap::new();

	for key in well_known_keys_list {
		if *key == well_known_keys::CURRENT_SLOT {
			// Use the new slot value
			updates.insert(key.to_vec(), Some(new_slot.to_le_bytes().to_vec()));
		} else if let Some(value) = entries.get(*key) {
			// Use the original value
			updates.insert(key.to_vec(), Some(value.clone()));
		}
		// Note: If key not found, we don't include it (Chopsticks does the same)
	}

	eprintln!(
		"[update_current_slot_minimal] Creating proof with {} well-known keys",
		updates.len()
	);

	// Create proof with ONLY the well-known keys as updates (empty base)
	// This creates a fresh minimal proof
	create_proof_from_entries(updates)
}

/// Create a new merkle proof from a set of key-value entries.
///
/// This creates a fresh proof from scratch, useful when you want to include
/// only specific keys rather than copying from an existing proof.
///
/// # Arguments
///
/// * `entries` - Key-value pairs to include in the proof.
///
/// # Returns
///
/// A tuple of (new_root_hash, new_proof_nodes).
pub fn create_proof_from_entries(
	entries: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
) -> Result<([u8; 32], Vec<Vec<u8>>), ProofError> {
	let mut trie = trie_structure::TrieStructure::<Vec<u8>>::new();

	// Insert all entries
	for (key, value) in &entries {
		if let Some(v) = value {
			let nibbles = bytes_to_nibbles(key.iter().cloned());
			match trie.node(nibbles) {
				trie_structure::Entry::Vacant(vacant) => {
					vacant.insert_storage_value().insert(v.clone(), vec![]);
				},
				trie_structure::Entry::Occupied(_) => {
					return Err(ProofError::EncodeFailed(format!(
						"Duplicate key: 0x{}",
						hex::encode(key)
					)));
				},
			}
		}
	}

	// Build the proof with Unhashed values (state version 0)
	let mut proof_builder = ProofBuilder::new();

	for node_index in trie.clone().iter_unordered() {
		let key: Vec<Nibble> = trie.node_full_key_by_index(node_index).unwrap().collect();

		let has_storage_value = trie.node_by_index(node_index).unwrap().has_storage_value();
		let storage_value = if has_storage_value {
			trie.node_by_index(node_index)
				.unwrap()
				.into_storage()
				.unwrap()
				.user_data()
				.clone()
		} else {
			vec![]
		};

		let decoded_node = trie_node::Decoded {
			children: std::array::from_fn(|nibble| {
				let nibble = Nibble::try_from(u8::try_from(nibble).unwrap()).unwrap();
				if trie.node_by_index(node_index).unwrap().child_user_data(nibble).is_some() {
					Some(&[][..])
				} else {
					None
				}
			}),
			partial_key: trie
				.node_by_index(node_index)
				.unwrap()
				.partial_key()
				.collect::<Vec<_>>()
				.into_iter(),
			storage_value: if has_storage_value {
				trie_node::StorageValue::Unhashed(&storage_value[..])
			} else {
				trie_node::StorageValue::None
			},
		};

		let node_value = trie_node::encode_to_vec(decoded_node)
			.map_err(|e| ProofError::EncodeFailed(format!("Failed to encode node: {:?}", e)))?;

		proof_builder.set_node_value(&key, &node_value, None);
	}

	if proof_builder.missing_node_values().next().is_some() {
		return Err(ProofError::EncodeFailed("Proof has missing node values".to_string()));
	}

	proof_builder.make_coherent();
	let trie_root_hash = proof_builder
		.trie_root_hash()
		.ok_or_else(|| ProofError::EncodeFailed("Failed to compute trie root hash".to_string()))?;

	let new_nodes: Vec<Vec<u8>> = proof_builder
		.build()
		.skip(1)
		.enumerate()
		.filter(|(i, _)| i % 2 != 0)
		.map(|(_, chunk)| chunk.as_ref().to_vec())
		.collect();

	eprintln!(
		"[create_proof_from_entries] Output: {} nodes, total {} bytes",
		new_nodes.len(),
		new_nodes.iter().map(|n| n.len()).sum::<usize>()
	);

	Ok((trie_root_hash, new_nodes))
}

/// Create a new merkle proof using native Substrate trie implementation.
///
/// This uses `sp-trie` and `sp-state-machine` to produce proofs in the exact format
/// expected by Polkadot SDK runtimes, matching the approach used by `RelayStateSproofBuilder`.
///
/// # Arguments
///
/// * `entries` - Key-value pairs to include in the proof.
///
/// # Returns
///
/// A tuple of (root_hash, proof_nodes).
pub fn create_proof_native(
	entries: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
) -> Result<([u8; 32], Vec<Vec<u8>>), ProofError> {
	use sp_trie::PrefixedMemoryDB;

	// Create a memory database with default root (like RelayStateSproofBuilder does)
	let (db, root) = PrefixedMemoryDB::<sp_core::Blake2Hasher>::default_with_root();
	// Try V0 first - relay chain proof format may use V0 for compatibility
	// V0 stores values inline, V1 hashes values >32 bytes
	let state_version = sp_core::storage::StateVersion::V0;
	let mut backend = sp_state_machine::TrieBackendBuilder::new(db, root).build();

	// Track all keys for proof generation
	let mut relevant_keys = Vec::new();

	// Insert all entries into the trie backend
	for (key, value) in &entries {
		if let Some(v) = value {
			relevant_keys.push(key.clone());
			// Insert using the backend's insert method
			// Format: vec![(child_info, vec![(key, value)])]
			// For top-level storage, child_info is None
			backend.insert(vec![(None, vec![(key.clone(), Some(v.clone()))])], state_version);
		}
	}

	// Get the new root after all insertions
	let root_h256 = *backend.root();
	let root: [u8; 32] = root_h256.into();

	eprintln!(
		"[create_proof_native] Created trie with {} entries, root: 0x{}",
		relevant_keys.len(),
		hex::encode(&root)
	);

	// Generate the proof using prove_read (same as RelayStateSproofBuilder)
	let proof = sp_state_machine::prove_read(backend, relevant_keys)
		.map_err(|e| ProofError::EncodeFailed(format!("Failed to prove read: {:?}", e)))?;

	// Convert StorageProof to Vec<Vec<u8>>
	let proof_nodes: Vec<Vec<u8>> = proof.iter_nodes().map(|n| n.clone()).collect();

	eprintln!(
		"[create_proof_native] Output: {} nodes, total {} bytes",
		proof_nodes.len(),
		proof_nodes.iter().map(|n| n.len()).sum::<usize>()
	);

	// Verify root hash exists in proof (same check as RelayChainStateProof::new)
	let root_in_proof = proof_nodes.iter().any(|node| {
		sp_core::blake2_256(node) == root
	});
	eprintln!(
		"[create_proof_native] Root hash present in proof: {}",
		root_in_proof
	);

	Ok((root, proof_nodes))
}

/// Update the CURRENT_SLOT in a relay chain state proof by patching in-place.
///
/// Unlike `create_proof`, this function preserves the original proof structure
/// by only modifying the nodes along the path from CURRENT_SLOT to the root.
///
/// # Arguments
///
/// * `nodes` - The original proof nodes
/// * `trie_root` - The current trie root hash
/// * `new_slot` - The new slot value to set
///
/// # Returns
///
/// A tuple of (new_root_hash, modified_proof_nodes).
pub fn update_current_slot_in_place(
	nodes: &[Vec<u8>],
	trie_root: &[u8; 32],
	new_slot: u64,
) -> Result<([u8; 32], Vec<Vec<u8>>), ProofError> {
	let mut updates = BTreeMap::new();
	updates.insert(
		well_known_keys::CURRENT_SLOT.to_vec(),
		new_slot.to_le_bytes().to_vec(),
	);
	update_proof_in_place(nodes, trie_root, updates)
}

/// Update multiple keys in a relay chain state proof by patching in-place.
///
/// Unlike `create_proof`, this function preserves the original proof structure
/// by only modifying the nodes along the paths from updated keys to the root.
/// This is critical for parachains where the runtime may traverse intermediate
/// branch nodes during proof verification.
///
/// # Arguments
///
/// * `nodes` - The original proof nodes
/// * `trie_root` - The current trie root hash
/// * `updates` - Map of key -> new_value for keys to update
///
/// # Returns
///
/// A tuple of (new_root_hash, modified_proof_nodes).
pub fn update_proof_in_place(
	nodes: &[Vec<u8>],
	trie_root: &[u8; 32],
	updates: BTreeMap<Vec<u8>, Vec<u8>>,
) -> Result<([u8; 32], Vec<Vec<u8>>), ProofError> {
	use std::collections::HashMap;

	if updates.is_empty() {
		return Ok((*trie_root, nodes.to_vec()));
	}

	eprintln!(
		"[update_proof_in_place] Starting with {} nodes, root 0x{}, {} updates",
		nodes.len(),
		hex::encode(trie_root),
		updates.len()
	);

	// Clone nodes for modification
	let mut new_nodes = nodes.to_vec();
	let mut current_root = *trie_root;

	// Apply each update sequentially
	// Each update may change the root, so we need to track it
	for (key, new_value) in &updates {
		let key_nibbles: Vec<Nibble> = bytes_to_nibbles(key.iter().copied()).collect();

		// Find path from current root to this key
		let path = match find_path_to_key(&new_nodes, &current_root, &key_nibbles) {
			Ok(p) => p,
			Err(e) => {
				eprintln!(
					"[update_proof_in_place] Failed to find path for key 0x{}: {}",
					hex::encode(key),
					e
				);
				return Err(e);
			},
		};

		if path.is_empty() {
			return Err(ProofError::KeyNotFound(key.clone()));
		}

		eprintln!(
			"[update_proof_in_place] Key 0x{} has path with {} nodes",
			hex::encode(&key[..std::cmp::min(8, key.len())]),
			path.len()
		);

		// Track which nodes have been modified by index
		let mut modified_nodes: HashMap<usize, Vec<u8>> = HashMap::new();

		// Start from the leaf and propagate changes up
		let mut current_child_data: Vec<u8>;

		// Process the leaf node first
		let leaf_entry = path.last().unwrap();
		if let Some(node_idx) = leaf_entry.node_index {
			// Standalone leaf node
			let leaf_node = &new_nodes[node_idx];
			let new_leaf_encoded = encode_node_with_new_value(leaf_node, new_value)?;

			// Check if the new leaf should be inline (< 32 bytes) or referenced by hash
			if new_leaf_encoded.len() < 32 {
				current_child_data = new_leaf_encoded;
			} else {
				modified_nodes.insert(node_idx, new_leaf_encoded.clone());
				current_child_data = sp_core::blake2_256(&new_leaf_encoded).to_vec();
			}
		} else {
			// Inline leaf - modify it
			let inline_data = leaf_entry.inline_data.as_ref().unwrap();
			let new_leaf_encoded = encode_node_with_new_value(inline_data, new_value)?;
			current_child_data = new_leaf_encoded;
		}

		// Propagate changes up the path (from leaf's parent to root)
		for i in (0..path.len() - 1).rev() {
			let path_entry = &path[i];
			let child_nibble =
				path_entry.child_nibble.expect("Non-leaf path nodes must have child_nibble");

			if let Some(node_idx) = path_entry.node_index {
				// Get the current version of this node (may have been modified already)
				let parent_node = modified_nodes
					.get(&node_idx)
					.unwrap_or(&new_nodes[node_idx]);
				let new_parent = encode_node_with_updated_child_data(
					parent_node,
					child_nibble,
					&current_child_data,
				)?;

				// Check if this node should now be inline
				if new_parent.len() < 32 && i > 0 {
					current_child_data = new_parent;
				} else {
					modified_nodes.insert(node_idx, new_parent.clone());
					current_child_data = sp_core::blake2_256(&new_parent).to_vec();
				}
			} else {
				// Inline node - update its child reference
				let inline_data = path_entry.inline_data.as_ref().unwrap();
				let new_node = encode_node_with_updated_child_data(
					inline_data,
					child_nibble,
					&current_child_data,
				)?;
				current_child_data = new_node;
			}
		}

		// The final current_child_data should be a 32-byte hash (the new root)
		if current_child_data.len() != 32 {
			return Err(ProofError::EncodeFailed(format!(
				"Expected 32-byte root hash, got {} bytes",
				current_child_data.len()
			)));
		}

		// Apply all modifications to new_nodes
		for (idx, data) in modified_nodes {
			new_nodes[idx] = data;
		}

		current_root = current_child_data.try_into().unwrap();
	}

	eprintln!(
		"[update_proof_in_place] New root: 0x{} (original: 0x{})",
		hex::encode(&current_root),
		hex::encode(trie_root)
	);

	// NOTE: Don't cleanup unreachable nodes. The old nodes with old hashes are still
	// needed because the proof structure requires ALL nodes that the runtime might
	// read from, not just the ones on the modified paths.
	// The runtime uses a sparse merkle proof verifier that reads values on-demand,
	// not a full verification of the entire proof structure.
	eprintln!(
		"[update_proof_in_place] Keeping all {} nodes (no cleanup)",
		new_nodes.len()
	);

	Ok((current_root, new_nodes))
}

/// Remove unreachable nodes from a proof.
///
/// After in-place updates, some nodes may become orphaned (e.g., when a node
/// transitions from standalone to inline). This function traverses from the
/// root and keeps only reachable nodes.
fn cleanup_unreachable_nodes(nodes: &[Vec<u8>], root_hash: &[u8; 32]) -> Vec<Vec<u8>> {
	use std::collections::{HashMap, HashSet};

	// Build hash -> index map
	let mut hash_to_index: HashMap<[u8; 32], usize> = HashMap::new();
	for (i, node) in nodes.iter().enumerate() {
		let hash = sp_core::blake2_256(node);
		hash_to_index.insert(hash, i);
	}

	// Find the root node
	let root_idx = match hash_to_index.get(root_hash) {
		Some(&idx) => idx,
		None => {
			eprintln!(
				"[cleanup_unreachable_nodes] Root hash not found, returning original nodes"
			);
			return nodes.to_vec();
		},
	};

	// BFS to find all reachable nodes
	let mut reachable: HashSet<usize> = HashSet::new();
	let mut queue: Vec<usize> = vec![root_idx];

	while let Some(idx) = queue.pop() {
		if reachable.contains(&idx) {
			continue;
		}
		reachable.insert(idx);

		// Decode the node and find child references
		let node = &nodes[idx];
		if let Ok(decoded) = trie_node::decode(node) {
			for child_opt in decoded.children.iter() {
				if let Some(child_ref) = child_opt {
					let child_bytes: &[u8] = child_ref.as_ref();
					// Only hash references (32 bytes) point to other nodes
					if child_bytes.len() == 32 {
						if let Ok(hash) = <[u8; 32]>::try_from(child_bytes) {
							if let Some(&child_idx) = hash_to_index.get(&hash) {
								queue.push(child_idx);
							}
						}
					}
				}
			}
		}
	}

	// Return only reachable nodes
	let cleaned: Vec<Vec<u8>> = nodes
		.iter()
		.enumerate()
		.filter(|(i, _)| reachable.contains(i))
		.map(|(_, n)| n.clone())
		.collect();

	cleaned
}

/// Entry in the path from root to a key.
#[derive(Debug)]
struct PathEntry {
	/// Index into the nodes array, or None if this is an inline node.
	node_index: Option<usize>,
	/// Nibble used to descend to the next node (None for leaf).
	child_nibble: Option<Nibble>,
	/// If this is an inline node, contains the inline node bytes.
	inline_data: Option<Vec<u8>>,
}

/// Find the path through the trie from root to a specific key.
/// Returns a path where each entry is either a standalone node (with node_index)
/// or an inline node (with inline_data).
fn find_path_to_key(
	nodes: &[Vec<u8>],
	trie_root: &[u8; 32],
	key_nibbles: &[Nibble],
) -> Result<Vec<PathEntry>, ProofError> {
	use std::collections::HashMap;

	// Build hash -> index map
	let mut hash_to_index: HashMap<[u8; 32], usize> = HashMap::new();
	for (i, node) in nodes.iter().enumerate() {
		let hash = sp_core::blake2_256(node);
		hash_to_index.insert(hash, i);
	}

	// Start from root
	let root_idx = hash_to_index.get(trie_root).ok_or_else(|| {
		ProofError::DecodeFailed(format!(
			"Root hash 0x{} not found in proof nodes",
			hex::encode(trie_root)
		))
	})?;

	let mut path = Vec::new();
	let mut current_node_data: Vec<u8> = nodes[*root_idx].clone();
	let mut current_node_idx: Option<usize> = Some(*root_idx);
	let mut key_offset = 0;

	loop {
		let decoded = trie_node::decode(&current_node_data).map_err(|e| {
			ProofError::DecodeFailed(format!("Failed to decode node: {:?}", e))
		})?;

		// Consume partial key
		let partial_key: Vec<Nibble> = decoded.partial_key.collect();
		if key_nibbles[key_offset..].starts_with(&partial_key) {
			key_offset += partial_key.len();
		} else {
			return Err(ProofError::KeyNotFound(
				nibbles_to_bytes_suffix_extend(key_nibbles.iter().copied()).collect(),
			));
		}

		// Check if we've reached the key
		if key_offset == key_nibbles.len() {
			// This is the leaf containing our value
			path.push(PathEntry {
				node_index: current_node_idx,
				child_nibble: None,
				inline_data: if current_node_idx.is_none() {
					Some(current_node_data.clone())
				} else {
					None
				},
			});
			break;
		}

		// Need to descend further
		let next_nibble = key_nibbles[key_offset];
		key_offset += 1;

		// Find child - get the child reference as bytes
		let child_ref = decoded.children[usize::from(u8::from(next_nibble))].ok_or_else(|| {
			ProofError::KeyNotFound(
				nibbles_to_bytes_suffix_extend(key_nibbles.iter().copied()).collect(),
			)
		})?;

		// Child reference might be inline (< 32 bytes) or a hash reference (32 bytes)
		// Copy to owned Vec immediately to avoid borrow issues
		let child_ref_slice: &[u8] = child_ref.as_ref();
		let child_bytes: Vec<u8> = child_ref_slice.to_vec();
		let child_len = child_bytes.len();

		// Add current node to path
		path.push(PathEntry {
			node_index: current_node_idx,
			child_nibble: Some(next_nibble),
			inline_data: if current_node_idx.is_none() {
				Some(current_node_data.clone())
			} else {
				None
			},
		});

		if child_len == 32 {
			// Hash reference - look up in nodes
			let hash: [u8; 32] = child_bytes.try_into().unwrap();
			let child_idx = *hash_to_index.get(&hash).ok_or_else(|| {
				ProofError::DecodeFailed(format!(
					"Child hash 0x{} not found in proof",
					hex::encode(&hash)
				))
			})?;
			current_node_data = nodes[child_idx].clone();
			current_node_idx = Some(child_idx);
		} else {
			// Inline node - the child is encoded directly
			eprintln!(
				"[find_path_to_key] Found inline child at nibble {:?} ({} bytes)",
				next_nibble,
				child_len
			);
			current_node_data = child_bytes;
			current_node_idx = None;
		}
	}

	Ok(path)
}

/// Encode a trie node with a new storage value.
/// Takes the raw node bytes and returns new encoded bytes with the value replaced.
fn encode_node_with_new_value(node_bytes: &[u8], new_value: &[u8]) -> Result<Vec<u8>, ProofError> {
	let decoded = trie_node::decode(node_bytes)
		.map_err(|e| ProofError::DecodeFailed(format!("Failed to decode node: {:?}", e)))?;

	// Collect partial key
	let partial_key: Vec<Nibble> = decoded.partial_key.collect();

	// Collect children - convert to owned data
	let children_data: Vec<Option<Vec<u8>>> = decoded
		.children
		.iter()
		.map(|opt| opt.as_ref().map(|c| {
			let bytes: &[u8] = c.as_ref();
			bytes.to_vec()
		}))
		.collect();

	// Create children array from owned data
	let children: [Option<&[u8]>; 16] =
		std::array::from_fn(|i| children_data[i].as_deref());

	let new_decoded = trie_node::Decoded {
		children,
		partial_key: partial_key.into_iter(),
		storage_value: trie_node::StorageValue::Unhashed(new_value),
	};

	trie_node::encode_to_vec(new_decoded)
		.map_err(|e| ProofError::EncodeFailed(format!("Failed to encode node: {:?}", e)))
}

/// Encode a trie node with an updated child reference (hash or inline data).
/// Takes the raw node bytes and returns new encoded bytes with one child replaced.
fn encode_node_with_updated_child_data(
	node_bytes: &[u8],
	child_nibble: Nibble,
	new_child_data: &[u8],
) -> Result<Vec<u8>, ProofError> {
	let decoded = trie_node::decode(node_bytes)
		.map_err(|e| ProofError::DecodeFailed(format!("Failed to decode node: {:?}", e)))?;

	let child_idx = usize::from(u8::from(child_nibble));

	// Collect partial key
	let partial_key: Vec<Nibble> = decoded.partial_key.collect();

	// Collect children, replacing the target child with new data
	let children_data: Vec<Option<Vec<u8>>> = decoded
		.children
		.iter()
		.enumerate()
		.map(|(i, opt)| {
			if i == child_idx {
				Some(new_child_data.to_vec())
			} else {
				opt.as_ref().map(|c| {
					let bytes: &[u8] = c.as_ref();
					bytes.to_vec()
				})
			}
		})
		.collect();

	// Create children array from owned data
	let children: [Option<&[u8]>; 16] =
		std::array::from_fn(|i| children_data[i].as_deref());

	// Preserve storage value
	let storage_value_data: Option<Vec<u8>> = match decoded.storage_value {
		trie_node::StorageValue::None => None,
		trie_node::StorageValue::Unhashed(v) => {
			let bytes: &[u8] = v.as_ref();
			Some(bytes.to_vec())
		},
		trie_node::StorageValue::Hashed(_) => {
			return Err(ProofError::EncodeFailed(
				"Hashed storage values not supported in child update".to_string(),
			));
		},
	};

	let storage_value = match &storage_value_data {
		Some(v) => trie_node::StorageValue::Unhashed(v.as_slice()),
		None => trie_node::StorageValue::None,
	};

	let new_decoded =
		trie_node::Decoded { children, partial_key: partial_key.into_iter(), storage_value };

	trie_node::encode_to_vec(new_decoded)
		.map_err(|e| ProofError::EncodeFailed(format!("Failed to encode node: {:?}", e)))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn encode_decode_scale_compact_roundtrip() {
		for value in [0, 1, 63, 64, 100, 16383, 16384, 1073741823] {
			let encoded = encode_scale_compact_usize(value);
			let (decoded, _) = decode_scale_compact_usize(&encoded).unwrap();
			assert_eq!(value, decoded, "roundtrip failed for value {}", value);
		}
	}

	#[test]
	fn well_known_keys_are_correct() {
		// Verify CURRENT_SLOT key matches the expected value
		assert_eq!(well_known_keys::CURRENT_SLOT.len(), 32);
		assert_eq!(
			hex::encode(well_known_keys::CURRENT_SLOT),
			"1cb6f36e027abb2091cfb5110ab5087f06155b3cd9a8c9e5e9a23fd5dc13a5ed"
		);
	}
}
