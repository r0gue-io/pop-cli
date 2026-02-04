// SPDX-License-Identifier: GPL-3.0

//! Relay chain state proof manipulation utilities.
//!
//! This module provides utilities for modifying relay chain state proofs
//! to support building blocks on forked parachains without access to the
//! real relay chain.
//!
//! The key operations involve reading from and modifying the relay chain
//! state proof to update values like `Paras::Heads(para_id)` so that the
//! parachain runtime's validation checks pass.

use scale::{Decode, Encode};
use sp_core::Blake2Hasher;
use sp_trie::{EMPTY_PREFIX, LayoutV1, MemoryDB, StorageProof, TrieDBMutBuilder, TrieHash};
use std::collections::BTreeSet;

/// Prefix for `Paras::Heads` storage on the relay chain.
/// This is `twox_128("Paras") ++ twox_128("Heads")`.
const PARAS_HEADS_PREFIX: [u8; 32] = [
	0xcd, 0x71, 0x0b, 0x30, 0xbd, 0x2e, 0xab, 0x03, 0x52, 0xdd, 0xcc, 0x26, 0x41, 0x7a, 0xa1, 0x94,
	0x1b, 0x3c, 0x25, 0x2f, 0xcb, 0x29, 0xd8, 0x8e, 0xff, 0x4f, 0x3d, 0xe5, 0xde, 0x44, 0x76, 0xc3,
];

/// Well-known storage key for `Babe::CurrentSlot` on the relay chain.
/// This is `twox_128("Babe") ++ twox_128("CurrentSlot")`.
/// Used by parachains to derive their slot from the relay chain.
pub const CURRENT_SLOT_KEY: [u8; 32] = [
	0x1c, 0xb6, 0xf3, 0x6e, 0x02, 0x7a, 0xbb, 0x20, 0x91, 0xcf, 0xb5, 0x11, 0x0a, 0xb5, 0x08, 0x7f,
	0x06, 0x15, 0x5b, 0x3c, 0xd9, 0xa8, 0xc9, 0xe5, 0xe9, 0xa2, 0x3f, 0xd5, 0xdc, 0x13, 0xa5, 0xed,
];

/// Construct the storage key for `Paras::Heads(para_id)`.
///
/// The key format is: prefix ++ twox_64(para_id) ++ para_id
pub fn paras_heads_key(para_id: u32) -> Vec<u8> {
	let para_id_encoded = para_id.encode();
	let hash = sp_core::twox_64(&para_id_encoded);

	PARAS_HEADS_PREFIX
		.iter()
		.chain(hash.iter())
		.chain(para_id_encoded.iter())
		.copied()
		.collect()
}

/// Type alias for the relay chain trie layout.
type RelayLayout = LayoutV1<Blake2Hasher>;

/// Error type for proof manipulation operations.
#[derive(Debug, Clone)]
pub enum ProofError {
	/// Failed to decode a value from the proof.
	DecodeError(String),
	/// Failed to modify the trie.
	TrieError(String),
	/// The storage root was not found in the proof.
	RootNotFound,
	/// A required storage key was not found in the proof.
	KeyNotFound(String),
}

impl std::fmt::Display for ProofError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			ProofError::DecodeError(msg) => write!(f, "Decode error: {}", msg),
			ProofError::TrieError(msg) => write!(f, "Trie error: {}", msg),
			ProofError::RootNotFound => write!(f, "Storage root not found in proof"),
			ProofError::KeyNotFound(key) => write!(f, "Key not found: {}", key),
		}
	}
}

impl std::error::Error for ProofError {}

/// Read a value from a storage proof.
///
/// # Arguments
///
/// * `proof` - The storage proof containing trie nodes
/// * `root` - The expected storage root
/// * `key` - The storage key to read
///
/// # Returns
///
/// The decoded value if found, or an error.
pub fn read_from_proof<T: Decode>(
	proof: &StorageProof,
	root: &[u8; 32],
	key: &[u8],
) -> Result<Option<T>, ProofError> {
	use sp_trie::TrieDBBuilder;
	use trie_db::Trie;

	let db: MemoryDB<Blake2Hasher> = proof.clone().into_memory_db();
	let root_hash = TrieHash::<RelayLayout>::from_slice(root);

	let trie = TrieDBBuilder::<RelayLayout>::new(&db, &root_hash).build();

	match trie.get(key) {
		Ok(Some(data)) => T::decode(&mut &data[..])
			.map(Some)
			.map_err(|e| ProofError::DecodeError(e.to_string())),
		Ok(None) => Ok(None),
		Err(e) => Err(ProofError::TrieError(format!("Failed to read from trie: {:?}", e))),
	}
}

/// Read raw bytes from a storage proof without decoding.
///
/// # Arguments
///
/// * `proof` - The storage proof containing trie nodes
/// * `root` - The expected storage root
/// * `key` - The storage key to read
///
/// # Returns
///
/// The raw bytes if found, or None.
pub fn read_raw_from_proof(
	proof: &StorageProof,
	root: &[u8; 32],
	key: &[u8],
) -> Result<Option<Vec<u8>>, ProofError> {
	use sp_trie::TrieDBBuilder;
	use trie_db::Trie;

	let db: MemoryDB<Blake2Hasher> = proof.clone().into_memory_db();
	let root_hash = TrieHash::<RelayLayout>::from_slice(root);

	let trie = TrieDBBuilder::<RelayLayout>::new(&db, &root_hash).build();

	match trie.get(key) {
		Ok(Some(data)) => Ok(Some(data)),
		Ok(None) => Ok(None),
		Err(e) => Err(ProofError::TrieError(format!("Failed to read from trie: {:?}", e))),
	}
}

/// Modify values in a storage proof and return the new root and proof.
///
/// This function:
/// 1. Creates a mutable trie from the proof
/// 2. Inserts/updates the specified key-value pairs
/// 3. Computes the new root
/// 4. Extracts the modified proof nodes
///
/// # Arguments
///
/// * `proof` - The original storage proof
/// * `root` - The original storage root
/// * `updates` - Iterator of (key, value) pairs to update
///
/// # Returns
///
/// A tuple of (new_root, new_proof) or an error.
pub fn modify_proof<'a, I>(
	proof: &StorageProof,
	root: &[u8; 32],
	updates: I,
) -> Result<([u8; 32], StorageProof), ProofError>
where
	I: IntoIterator<Item = (&'a [u8], Vec<u8>)>,
{
	let mut db: MemoryDB<Blake2Hasher> = proof.clone().into_memory_db();
	let mut root_hash = TrieHash::<RelayLayout>::from_slice(root);

	// Build a mutable trie and apply updates
	{
		use sp_trie::TrieMut;
		let mut trie =
			TrieDBMutBuilder::<RelayLayout>::from_existing(&mut db, &mut root_hash).build();

		for (key, value) in updates {
			eprintln!(
				"[RelayProof] Updating key 0x{}... with {} bytes",
				hex::encode(&key[..8.min(key.len())]),
				value.len()
			);
			trie.insert(key, &value)
				.map_err(|e| ProofError::TrieError(format!("Failed to insert: {:?}", e)))?;
		}

		// Commit changes
		trie.commit();
	}

	// Extract the new proof from the modified database
	let new_proof = extract_proof_from_db(&db);

	eprintln!(
		"[RelayProof] New storage root: 0x{}",
		hex::encode(&root_hash)
	);

	Ok((root_hash.into(), new_proof))
}

/// Extract a StorageProof from a MemoryDB.
///
/// This collects all trie nodes from the database into a proof.
fn extract_proof_from_db(db: &MemoryDB<Blake2Hasher>) -> StorageProof {
	use sp_trie::HashDBT;
	let mut nodes = BTreeSet::new();

	// MemoryDB stores nodes by their hash. We need to iterate and collect all nodes.
	for (key, (value, rc)) in db.clone().drain() {
		if rc > 0 {
			nodes.insert(value);
		}
		// Also try to get the node directly
		if let Some(data) = db.get(&key, EMPTY_PREFIX) {
			nodes.insert(data);
		}
	}

	StorageProof::new(nodes)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn paras_heads_prefix_is_correct() {
		// Verify the prefix matches twox_128("Paras") ++ twox_128("Heads")
		let pallet_hash = sp_core::twox_128(b"Paras");
		let storage_hash = sp_core::twox_128(b"Heads");

		let expected: Vec<u8> = pallet_hash.iter().chain(storage_hash.iter()).copied().collect();

		assert_eq!(PARAS_HEADS_PREFIX.to_vec(), expected);
	}

	#[test]
	fn current_slot_key_is_correct() {
		// Verify the key matches twox_128("Babe") ++ twox_128("CurrentSlot")
		let pallet_hash = sp_core::twox_128(b"Babe");
		let storage_hash = sp_core::twox_128(b"CurrentSlot");

		let expected: Vec<u8> = pallet_hash.iter().chain(storage_hash.iter()).copied().collect();

		assert_eq!(CURRENT_SLOT_KEY.to_vec(), expected);
	}

	#[test]
	fn paras_heads_key_format_is_correct() {
		let para_id: u32 = 1000;
		let key = paras_heads_key(para_id);

		// Key should be: prefix (32) + twox_64 hash (8) + para_id encoded (4) = 44 bytes
		assert_eq!(key.len(), 44);

		// First 32 bytes should be the prefix
		assert_eq!(&key[..32], &PARAS_HEADS_PREFIX[..]);

		// Next 8 bytes should be twox_64 of encoded para_id
		let para_id_encoded = para_id.encode();
		let expected_hash = sp_core::twox_64(&para_id_encoded);
		assert_eq!(&key[32..40], &expected_hash[..]);

		// Last 4 bytes should be the encoded para_id
		assert_eq!(&key[40..], &para_id_encoded[..]);
	}
}