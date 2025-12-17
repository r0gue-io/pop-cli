// SPDX-License-Identifier: GPL-3.0

use crate::{ForkRpcClient, error::MetadataError};
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
	}
}
