// SPDX-License-Identifier: GPL-3.0

use crate::{ForkRpcClient, error::MetadataError};
use pop_chains::Pallet;
use sp_core::hashing::twox_128;
use subxt::{Metadata, ext::codec::Decode};

/// Wrapper around [`Metadata`] for forking specific operations
#[derive(Debug)]
pub struct ForkMetadata(Metadata);

impl ForkMetadata {
	/// Create `ForkMetadata` from an RPC client by fetching metadata at the finalized head.
	///
	/// This is a convenience method that:
	/// 1. Fetches the finalized head block hash
	/// 2. Retrieves metadata at that block
	/// 3. Decodes it into `ForkMetadata`
	///
	/// # Arguments
	/// - client - The ForkRpcClient
	pub async fn from_rpc_client(client: &ForkRpcClient) -> Result<Self, MetadataError> {
		let hash = client.finalized_head().await?;
		let metadata_bytes = client.metadata(hash).await?;
		Self::try_from(metadata_bytes)
	}

	/// Parse the chain metadata to extract pallets and their callable items.
	pub fn pallets(&self) -> Result<Vec<Pallet>, MetadataError> {
		pop_chains::parse_chain_metadata(&self.0).map_err(|e| {
			MetadataError::RpcError(crate::error::RpcClientError::InvalidResponse(format!(
				"Failed to parse metadata: {}",
				e
			)))
		})
	}

	/// Generate storage key prefix for a pallet/item combination.
	///
	/// This method is used by the storage layer to generate the storage key prefix
	/// that identifies a specific storage item within a pallet. The key is generated
	/// using the xxHash128 algorithm on both the pallet and item names.
	///
	/// # Arguments
	/// * `pallet` - The name of the pallet
	/// * `item` - The name of the storage item
	/// ```
	pub fn storage_prefix(&self, pallet: &str, item: &str) -> Result<Vec<u8>, MetadataError> {
		let mut key = Vec::with_capacity(32);
		key.extend_from_slice(&twox_128(pallet.as_bytes()));
		key.extend_from_slice(&twox_128(item.as_bytes()));
		Ok(key)
	}

	/// Get a reference to the inner [`Metadata`].
	///
	/// This provides access to the underlying subxt `Metadata` for advanced operations.
	pub fn inner(&self) -> &Metadata {
		&self.0
	}
}

impl TryFrom<&[u8]> for ForkMetadata {
	type Error = MetadataError;

	fn try_from(mut bytes: &[u8]) -> Result<Self, Self::Error> {
		let metadata = Metadata::decode(&mut bytes).map_err(|_| MetadataError::DecodeError)?;
		Ok(Self(metadata))
	}
}

impl TryFrom<Vec<u8>> for ForkMetadata {
	type Error = MetadataError;

	fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
		let metadata = Metadata::decode(&mut &bytes[..]).map_err(|_| MetadataError::DecodeError)?;
		Ok(Self(metadata))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn tryfrom_slice_with_invalid_bytes_fails() {
		let random_bytes: &[u8] = &[0x01, 0x02, 0x03, 0x04];
		let result = ForkMetadata::try_from(random_bytes);
		assert!(result.is_err());
		assert!(matches!(result.unwrap_err(), MetadataError::DecodeError));
	}

	#[test]
	fn tryfrom_vec_with_invalid_bytes_fails() {
		let random_bytes: Vec<u8> = vec![0xff, 0xaa, 0xbb, 0xcc];
		let result = ForkMetadata::try_from(random_bytes);
		assert!(result.is_err());
		assert!(matches!(result.unwrap_err(), MetadataError::DecodeError));
	}

	mod sequential {
		use super::*;
		use pop_common::test_env::TestNode;

		#[tokio::test]
		async fn tryfrom_slice_with_valid_metadata_succeeds() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint = node.ws_url().parse().expect("Invalid URL");
			let client = crate::ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();
			let metadata_bytes = client.metadata(hash).await.unwrap();

			// Test with slice reference
			let result = ForkMetadata::try_from(metadata_bytes.as_slice());
			assert!(result.is_ok(), "TryFrom<&[u8]> should succeed with valid metadata");
		}

		#[tokio::test]
		async fn tryfrom_vec_with_valid_metadata_succeeds() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint = node.ws_url().parse().expect("Invalid URL");
			let client = crate::ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();
			let metadata_bytes = client.metadata(hash).await.unwrap();

			// Test with Vec<u8>
			let result = ForkMetadata::try_from(metadata_bytes);
			assert!(result.is_ok(), "TryFrom<Vec<u8>> should succeed with valid metadata");
		}

		#[tokio::test]
		async fn from_rpc_client_succeeds() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint = node.ws_url().parse().expect("Invalid URL");
			let client = crate::ForkRpcClient::connect(&endpoint).await.unwrap();

			// Test creating ForkMetadata directly from RPC client
			let result = ForkMetadata::from_rpc_client(&client).await;
			assert!(result.is_ok(), "from_rpc_client should succeed with valid RPC client");
		}

		#[tokio::test]
		async fn storage_prefix_generates_correct_key() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint = node.ws_url().parse().expect("Invalid URL");
			let client = crate::ForkRpcClient::connect(&endpoint).await.unwrap();
			let metadata = ForkMetadata::from_rpc_client(&client).await.unwrap();

			// Generate storage key for System::Account
			let key = metadata.storage_prefix("System", "Account").unwrap();

			// The key should be exactly 32 bytes
			assert_eq!(key.len(), 32, "Storage prefix should be 32 bytes");

			// Verify it matches manually computed key
			let expected_key = {
				let mut k = Vec::with_capacity(32);
				k.extend_from_slice(&twox_128(b"System"));
				k.extend_from_slice(&twox_128(b"Account"));
				k
			};
			assert_eq!(key, expected_key, "Storage prefix should match expected key");
		}

		#[tokio::test]
		async fn pallets_returns_valid_data() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint = node.ws_url().parse().expect("Invalid URL");
			let client = crate::ForkRpcClient::connect(&endpoint).await.unwrap();
			let metadata = ForkMetadata::from_rpc_client(&client).await.unwrap();

			// Parse pallets
			let pallets = metadata.pallets().unwrap();

			// Should have at least some standard pallets
			assert!(!pallets.is_empty(), "Should have at least one pallet");

			// Should find System pallet
			let system_pallet = pallets.iter().find(|p| p.name == "System");
			assert!(system_pallet.is_some(), "System pallet should exist");

			let system = system_pallet.unwrap();
			assert!(!system.functions.is_empty(), "System should have functions");
			assert!(!system.state.is_empty(), "System should have storage items");
		}
	}
}
