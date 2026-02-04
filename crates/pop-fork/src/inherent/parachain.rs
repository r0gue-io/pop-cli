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
//! 2. Decodes the validation data and relay chain state proof
//! 3. Modifies the proof to update `Paras::Heads(para_id)` with the parachain's head
//! 4. Regenerates the storage root to match the updated proof
//! 5. Re-encodes the extrinsic with all updated data
//!
//! # Why Proof Modification is Needed
//!
//! The parachain runtime validates that its head appears in the relay chain
//! state proof at `Paras::Heads(para_id)`. When forking, the original proof
//! contains the old head, so we update it with our latest block header to
//! make the validation pass.
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::inherent::ParachainInherent;
//!
//! let provider = ParachainInherent::new();
//! ```

use super::relay_proof;
use crate::{
	Block, BlockBuilderError, DigestItem, RuntimeExecutor, consensus_engine,
	inherent::InherentProvider,
	strings::{executor::magic_signature, inherent::parachain as strings},
};
use async_trait::async_trait;
use scale::{Compact, Decode, Encode};
use sp_core::blake2_256;
use sp_trie::StorageProof;
use std::collections::BTreeSet;

/// Extrinsic format version for unsigned/bare extrinsics (v5 - new format).
const EXTRINSIC_FORMAT_VERSION_V5: u8 = 5;
/// Extrinsic format version for unsigned/bare extrinsics (v4 - legacy format).
const EXTRINSIC_FORMAT_VERSION_V4: u8 = 4;

// ============================================================================
// Types for decoding/encoding the inherent data
// ============================================================================

/// Persisted validation data from the relay chain.
#[derive(Debug, Clone, Encode, Decode)]
struct PersistedValidationData {
	/// Parachain head data (parent block header).
	parent_head: Vec<u8>,
	/// Relay chain block number.
	relay_parent_number: u32,
	/// Storage root of the relay chain at `relay_parent_number`.
	relay_parent_storage_root: [u8; 32],
	/// Maximum proof-of-validity size.
	max_pov_size: u32,
}

/// Storage proof from the relay chain (just a set of trie nodes).
#[derive(Debug, Clone)]
struct RelayChainStateProof {
	trie_nodes: BTreeSet<Vec<u8>>,
}

impl Encode for RelayChainStateProof {
	fn encode(&self) -> Vec<u8> {
		// Encode as Vec<Vec<u8>> (sorted order from BTreeSet)
		let nodes: Vec<Vec<u8>> = self.trie_nodes.iter().cloned().collect();
		nodes.encode()
	}
}

impl Decode for RelayChainStateProof {
	fn decode<I: scale::Input>(input: &mut I) -> Result<Self, scale::Error> {
		let nodes: Vec<Vec<u8>> = Decode::decode(input)?;
		Ok(Self { trie_nodes: nodes.into_iter().collect() })
	}
}

impl From<StorageProof> for RelayChainStateProof {
	fn from(proof: StorageProof) -> Self {
		Self { trie_nodes: proof.into_nodes() }
	}
}

impl From<RelayChainStateProof> for StorageProof {
	fn from(proof: RelayChainStateProof) -> Self {
		StorageProof::new(proof.trie_nodes)
	}
}

/// Relay chain header for descendant validation.
///
/// This structure matches the header format expected by `parachain-system`
/// for validating relay parent descendants.
#[derive(Debug, Clone, Encode, Decode)]
struct RelayHeader {
	parent_hash: [u8; 32],
	#[codec(compact)]
	number: u32,
	state_root: [u8; 32],
	extrinsics_root: [u8; 32],
	digest: Digest,
}

/// Digest containing log items for a relay chain header.
#[derive(Debug, Clone, Encode, Decode)]
struct Digest {
	logs: Vec<DigestItem>,
}

impl RelayHeader {
	/// Compute the blake2-256 hash of this header.
	fn hash(&self) -> [u8; 32] {
		blake2_256(&self.encode())
	}

	/// Replace the BABE seal with a magic signature.
	///
	/// This allows the signature to pass validation when `MagicSignature` mode is enabled.
	fn replace_seal_with_magic(&mut self) {
		for item in self.digest.logs.iter_mut() {
			if let DigestItem::Seal(engine_id, signature) = item &&
				*engine_id == consensus_engine::BABE
			{
				// Create magic signature: 0xdeadbeef + padding to fill sr25519 size
				let mut magic_sig = magic_signature::PREFIX.to_vec();
				magic_sig.extend(std::iter::repeat_n(
					magic_signature::PADDING,
					magic_signature::SR25519_SIZE - magic_signature::PREFIX.len(),
				));
				*signature = magic_sig;
			}
		}
	}
}

/// Parsed inherent data with typed relay_parent_descendants.
struct ParsedInherentData {
	validation_data: PersistedValidationData,
	relay_chain_state: RelayChainStateProof,
	relay_parent_descendants: Vec<RelayHeader>,
	collator_peer_id: Option<Vec<u8>>,
	/// Remaining bytes after collator_peer_id (e.g., InboundMessagesData in v5 format).
	remaining: Vec<u8>,
}

// ============================================================================
// ParachainInherent Provider
// ============================================================================

/// Parachain inherent provider.
///
/// Generates the `parachainSystem.setValidationData` inherent extrinsic
/// that provides relay chain validation data to the parachain runtime.
#[derive(Debug, Clone, Default)]
pub struct ParachainInherent;

impl ParachainInherent {
	/// Create a new parachain inherent provider.
	pub fn new() -> Self {
		Self
	}

	/// Compute the storage key for `ParachainInfo::ParachainId`.
	fn parachain_id_key() -> Vec<u8> {
		let pallet_hash = sp_core::twox_128(b"ParachainInfo");
		let storage_hash = sp_core::twox_128(b"ParachainId");
		[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
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

	/// Find the setValidationData extrinsic in the parent block.
	fn find_validation_data_extrinsic(
		extrinsics: &[Vec<u8>],
		pallet_index: u8,
		call_index: u8,
	) -> Option<&Vec<u8>> {
		for (i, ext) in extrinsics.iter().enumerate() {
			let Some((_len, remainder)) = decode_compact_len(ext) else {
				eprintln!("[ParachainInherent] ext[{}]: failed to decode compact length", i);
				continue;
			};

			if remainder.is_empty() {
				continue;
			}

			let version = remainder[0];
			// Accept both v4 (legacy) and v5 (new) extrinsic formats
			let is_valid_version =
				version == EXTRINSIC_FORMAT_VERSION_V4 || version == EXTRINSIC_FORMAT_VERSION_V5;

			if !is_valid_version {
				eprintln!(
					"[ParachainInherent] ext[{}]: version {} not recognized (expected 4 or 5)",
					i, version
				);
				continue;
			}

			if remainder.len() >= 3 && remainder[1] == pallet_index && remainder[2] == call_index {
				eprintln!(
					"[ParachainInherent] ext[{}]: MATCH! pallet={}, call={}, version={}",
					i, pallet_index, call_index, version
				);
				return Some(ext);
			}
		}
		None
	}

	/// Parse the extrinsic to extract validation data, proof, and relay parent descendants.
	fn parse_inherent_data(call_data: &[u8]) -> Result<ParsedInherentData, BlockBuilderError> {
		let mut cursor = call_data;

		// Decode PersistedValidationData (first field of BasicParachainInherentData)
		let validation_data = PersistedValidationData::decode(&mut cursor).map_err(|e| {
			BlockBuilderError::InherentProvider {
				provider: "ParachainSystem".to_string(),
				message: format!("Failed to decode PersistedValidationData: {}", e),
			}
		})?;

		// Decode relay_chain_state (StorageProof)
		let relay_chain_state = RelayChainStateProof::decode(&mut cursor).map_err(|e| {
			BlockBuilderError::InherentProvider {
				provider: "ParachainSystem".to_string(),
				message: format!("Failed to decode relay_chain_state: {}", e),
			}
		})?;

		// Decode relay_parent_descendants (Vec<RelayHeader>)
		let relay_parent_descendants: Vec<RelayHeader> =
			Decode::decode(&mut cursor).map_err(|e| BlockBuilderError::InherentProvider {
				provider: "ParachainSystem".to_string(),
				message: format!("Failed to decode relay_parent_descendants: {}", e),
			})?;

		// Decode collator_peer_id (Option<Vec<u8>>)
		let collator_peer_id: Option<Vec<u8>> =
			Decode::decode(&mut cursor).map_err(|e| BlockBuilderError::InherentProvider {
				provider: "ParachainSystem".to_string(),
				message: format!("Failed to decode collator_peer_id: {}", e),
			})?;

		// Capture any remaining bytes (e.g., InboundMessagesData in v5 format)
		let remaining = cursor.to_vec();

		Ok(ParsedInherentData {
			validation_data,
			relay_chain_state,
			relay_parent_descendants,
			collator_peer_id,
			remaining,
		})
	}

	/// Process relay_parent_descendants to match updated storage root.
	///
	/// For chains with `RELAY_PARENT_OFFSET > 0` (like AssetHub-Paseo with offset=2),
	/// the runtime validates that the first descendant's `state_root` matches
	/// `relay_parent_storage_root`. When we modify the proof, we must also update
	/// the first header's `state_root` and re-chain the subsequent headers.
	fn process_relay_parent_descendants(
		mut descendants: Vec<RelayHeader>,
		new_storage_root: [u8; 32],
	) -> Vec<RelayHeader> {
		if descendants.is_empty() {
			return descendants;
		}

		eprintln!("[ParachainInherent] Processing {} relay parent descendants", descendants.len());

		// 1. Update first header's state_root to match the new storage root
		descendants[0].state_root = new_storage_root;
		descendants[0].replace_seal_with_magic();

		// 2. Re-chain: update parentHash for subsequent headers
		let mut prev_hash = descendants[0].hash();
		for header in descendants.iter_mut().skip(1) {
			header.parent_hash = prev_hash;
			header.replace_seal_with_magic();
			prev_hash = header.hash();
		}

		descendants
	}

	/// Process the inherent: update proof, storage root, and relay parent descendants.
	fn process_inherent(
		&self,
		ext: &[u8],
		para_id: u32,
		para_head: &[u8],
	) -> Result<Vec<u8>, BlockBuilderError> {
		// Decode the extrinsic structure
		let (_, body) =
			decode_compact_len(ext).ok_or_else(|| BlockBuilderError::InherentProvider {
				provider: "ParachainSystem".to_string(),
				message: "Failed to decode extrinsic length prefix".to_string(),
			})?;

		// body[0] = version, body[1] = pallet, body[2] = call
		let version = body[0];
		let pallet = body[1];
		let call = body[2];
		let call_data = &body[3..];

		// Parse the inherent data
		let parsed = Self::parse_inherent_data(call_data)?;
		let mut validation_data = parsed.validation_data;
		let relay_chain_state = parsed.relay_chain_state;
		let relay_parent_descendants = parsed.relay_parent_descendants;
		let collator_peer_id = parsed.collator_peer_id;
		let remaining = parsed.remaining;

		eprintln!(
			"[ParachainInherent] Decoded: relay_parent_number={}, storage_root=0x{}, proof_nodes={}, descendants={}, remaining={}",
			validation_data.relay_parent_number,
			hex::encode(validation_data.relay_parent_storage_root),
			relay_chain_state.trie_nodes.len(),
			relay_parent_descendants.len(),
			remaining.len()
		);

		// Convert to sp_trie::StorageProof for manipulation
		let proof: StorageProof = relay_chain_state.into();

		// Read the current relay slot from the proof
		let current_relay_slot: u64 = relay_proof::read_from_proof(
			&proof,
			&validation_data.relay_parent_storage_root,
			&relay_proof::CURRENT_SLOT_KEY,
		)
		.map_err(|e| BlockBuilderError::InherentProvider {
			provider: "ParachainSystem".to_string(),
			message: format!("Failed to read current slot from proof: {}", e),
		})?
		.ok_or_else(|| BlockBuilderError::InherentProvider {
			provider: "ParachainSystem".to_string(),
			message: "CURRENT_SLOT not found in relay chain proof".to_string(),
		})?;

		// Increment relay slot by 2 (12s para block / 6s relay slot = 2)
		// This ensures the derived para slot matches the timestamp we'll set
		let new_relay_slot = current_relay_slot.saturating_add(2);

		eprintln!("[ParachainInherent] Relay slot: {} -> {}", current_relay_slot, new_relay_slot);

		// Construct the Paras::Heads(para_id) key
		let heads_key = relay_proof::paras_heads_key(para_id);

		eprintln!(
			"[ParachainInherent] Updating Paras::Heads({}) with {} bytes",
			para_id,
			para_head.len()
		);

		// The value is the HeadData which is just the encoded header wrapped in a Vec
		let head_data = para_head.to_vec().encode();

		// Update both keys in a single modify_proof call
		let updates: Vec<(&[u8], Vec<u8>)> = vec![
			(&heads_key[..], head_data),
			(&relay_proof::CURRENT_SLOT_KEY[..], new_relay_slot.encode()),
		];

		let (new_root, new_proof) = relay_proof::modify_proof(
			&proof,
			&validation_data.relay_parent_storage_root,
			updates.into_iter(),
		)
		.map_err(|e| BlockBuilderError::InherentProvider {
			provider: "ParachainSystem".to_string(),
			message: format!("Failed to modify relay proof: {}", e),
		})?;

		// Update validation data with new storage root
		validation_data.relay_parent_storage_root = new_root;

		eprintln!("[ParachainInherent] Updated storage_root=0x{}", hex::encode(new_root));

		// Process relay parent descendants to match the new storage root
		// This is required for chains with RELAY_PARENT_OFFSET > 0 (e.g., AssetHub-Paseo)
		let processed_descendants =
			Self::process_relay_parent_descendants(relay_parent_descendants, new_root);

		// Convert new proof back to our type
		let new_relay_state: RelayChainStateProof = new_proof.into();

		// Re-encode the extrinsic
		let mut new_call_data = Vec::new();
		new_call_data.extend(validation_data.encode());
		new_call_data.extend(new_relay_state.encode());
		new_call_data.extend(processed_descendants.encode());
		new_call_data.extend(collator_peer_id.encode());
		// Append any remaining bytes (e.g., InboundMessagesData in v5 format)
		new_call_data.extend(&remaining);

		// Build new body
		let mut new_body = vec![version, pallet, call];
		new_body.extend(new_call_data);

		// Encode with compact length prefix
		let mut result = Compact(new_body.len() as u32).encode();
		result.extend(new_body);

		eprintln!(
			"[ParachainInherent] Re-encoded extrinsic: {} bytes (was {} bytes)",
			result.len(),
			ext.len()
		);

		Ok(result)
	}
}

/// Decode a compact length prefix from SCALE-encoded data.
fn decode_compact_len(data: &[u8]) -> Option<(u32, &[u8])> {
	if data.is_empty() {
		return None;
	}

	let first_byte = data[0];
	let mode = first_byte & 0b11;

	match mode {
		0b00 => {
			let len = (first_byte >> 2) as u32;
			Some((len, &data[1..]))
		},
		0b01 => {
			if data.len() < 2 {
				return None;
			}
			let len = (u16::from_le_bytes([data[0], data[1]]) >> 2) as u32;
			Some((len, &data[2..]))
		},
		0b10 => {
			if data.len() < 4 {
				return None;
			}
			let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) >> 2;
			Some((len, &data[4..]))
		},
		_ => None,
	}
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
		// Check if ParachainSystem pallet exists in metadata
		let metadata = parent.metadata().await?;

		let pallet = match metadata.pallet_by_name(strings::metadata::PALLET_NAME) {
			Some(p) => p,
			None => {
				// No ParachainSystem pallet - not a parachain runtime
				return Ok(vec![]);
			},
		};

		let pallet_index = pallet.index();

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

		// Read the parachain ID from storage
		let para_id = Self::read_parachain_id(parent).await.ok_or_else(|| {
			BlockBuilderError::InherentProvider {
				provider: self.identifier().to_string(),
				message: "Failed to read ParachainId from storage".to_string(),
			}
		})?;
		eprintln!("[ParachainInherent] ParachainId from storage: {}", para_id);

		// Find the setValidationData extrinsic in the parent block
		let validation_ext =
			Self::find_validation_data_extrinsic(&parent.extrinsics, pallet_index, call_index);

		match validation_ext {
			Some(ext) => {
				eprintln!(
					"[ParachainInherent] Found setValidationData extrinsic ({} bytes)",
					ext.len()
				);

				// The para head is the parent block's header.
				// We inject this into the relay chain proof at Paras::Heads(para_id)
				// so the parachain runtime's validation check finds our block.
				eprintln!(
					"[ParachainInherent] Parent block header: {} bytes, hash=0x{}",
					parent.header.len(),
					hex::encode(blake2_256(&parent.header))
				);

				// Process the inherent: update proof with our para head
				let processed = self.process_inherent(ext, para_id, &parent.header)?;

				Ok(vec![processed])
			},
			None => {
				eprintln!(
					"[ParachainInherent] No setValidationData extrinsic found in parent block"
				);
				Ok(vec![])
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn identifier_returns_parachain_system() {
		let provider = ParachainInherent::default();
		assert_eq!(provider.identifier(), strings::IDENTIFIER);
	}

	#[test]
	fn decode_compact_len_single_byte() {
		let data = [0x18, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
		let (len, remainder) = decode_compact_len(&data).unwrap();
		assert_eq!(len, 6);
		assert_eq!(remainder.len(), 6);
	}

	#[test]
	fn decode_compact_len_two_byte() {
		let data = [0x91, 0x01, 0x00, 0x00];
		let (len, remainder) = decode_compact_len(&data).unwrap();
		assert_eq!(len, 100);
		assert_eq!(remainder.len(), 2);
	}

	#[test]
	fn find_validation_data_extrinsic_finds_matching() {
		let mut ext = Compact(10u32).encode();
		ext.push(EXTRINSIC_FORMAT_VERSION_V4);
		ext.push(51);
		ext.push(0);
		ext.extend([0u8; 7]);

		let extrinsics = vec![ext.clone()];
		let result = ParachainInherent::find_validation_data_extrinsic(&extrinsics, 51, 0);
		assert!(result.is_some());
	}

	#[test]
	fn relay_chain_state_proof_roundtrip() {
		let mut nodes = BTreeSet::new();
		nodes.insert(vec![1, 2, 3]);
		nodes.insert(vec![4, 5, 6]);

		let proof = RelayChainStateProof { trie_nodes: nodes.clone() };
		let encoded = proof.encode();
		let decoded = RelayChainStateProof::decode(&mut &encoded[..]).unwrap();

		assert_eq!(decoded.trie_nodes, nodes);
	}

	#[test]
	fn persisted_validation_data_roundtrip() {
		let data = PersistedValidationData {
			parent_head: vec![1, 2, 3, 4],
			relay_parent_number: 12345,
			relay_parent_storage_root: [0xab; 32],
			max_pov_size: 5_000_000,
		};

		let encoded = data.encode();
		let decoded = PersistedValidationData::decode(&mut &encoded[..]).unwrap();

		assert_eq!(decoded.parent_head, vec![1, 2, 3, 4]);
		assert_eq!(decoded.relay_parent_number, 12345);
		assert_eq!(decoded.relay_parent_storage_root, [0xab; 32]);
		assert_eq!(decoded.max_pov_size, 5_000_000);
	}
}
