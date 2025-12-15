// SPDX-License-Identifier: GPL-3.0

//! Integration tests for the RPC client.
//!
//! These tests are separated from unit tests because they spawn local test nodes
//! (ink-node) which requires downloading binaries and starting external processes.
//!
//! # Why Integration Tests?
//!
//! 1. **External Dependencies**: Tests spawn real blockchain nodes, which is slow and requires
//!    network access to download binaries on first run.
//!
//! 2. **CI Isolation**: Keeping these separate allows CI to run them with special flags (like `-j
//!    1` for sequential execution) without affecting other tests.
//!
//! # Running These Tests
//!
//! ```bash
//! # Run with the integration-tests feature enabled
//! cargo nextest run -p pop-fork --features integration-tests --test rpc
//!
//! # For reliable execution, run sequentially to avoid concurrent node downloads
//! cargo nextest run -p pop-fork --features integration-tests --test rpc -j 1
//! ```

#![cfg(feature = "integration-tests")]

use pop_common::test_env::TestNode;
use pop_fork::{ForkRpcClient, RpcClientError};
use subxt::config::substrate::H256;
use url::Url;

// Well-known storage keys for testing.
// These are derived from twox128 hashes of pallet and storage item names.

/// System pallet prefix: twox128("System")
const SYSTEM_PALLET_PREFIX: &str = "26aa394eea5630e07c48ae0c9558cef7";

/// System::Number storage key: twox128("System") ++ twox128("Number")
const SYSTEM_NUMBER_KEY: &str = "26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac";

/// System::ParentHash storage key: twox128("System") ++ twox128("ParentHash")
const SYSTEM_PARENT_HASH_KEY: &str =
	"26aa394eea5630e07c48ae0c9558cef734abf5cb34d6244378cddbf18e849d96";

#[tokio::test]
async fn connect_to_node() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	assert_eq!(client.endpoint(), &endpoint);
}

#[tokio::test]
async fn fetch_finalized_head() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();
	// Hash should be 32 bytes
	assert_eq!(hash.as_bytes().len(), 32);
}

#[tokio::test]
async fn fetch_header() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();
	let header = client.header(hash).await.unwrap();
	// Header should have a valid state root (32 bytes)
	assert_eq!(header.state_root.as_bytes().len(), 32);
}

#[tokio::test]
async fn fetch_storage() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();

	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let value = client.storage(&key, hash).await.unwrap();

	// System::Number should exist and have a value
	assert!(value.is_some());
}

#[tokio::test]
async fn fetch_metadata() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();
	let metadata = client.metadata(hash).await.unwrap();

	// Metadata should be substantial
	assert!(metadata.len() > 1000);
}

#[tokio::test]
async fn fetch_runtime_code() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();
	let code = client.runtime_code(hash).await.unwrap();

	// Runtime code should be substantial
	// ink-node runtime is smaller than relay chains but still significant
	assert!(code.len() > 10_000, "Runtime code should be substantial, got {} bytes", code.len());
}

#[tokio::test]
async fn fetch_storage_keys_paged() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();

	let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();
	let keys = client.storage_keys_paged(&prefix, 10, None, hash).await.unwrap();

	// Should find some System storage keys
	assert!(!keys.is_empty());
	// All keys should start with the prefix
	for key in &keys {
		assert!(key.starts_with(&prefix));
	}
}

#[tokio::test]
async fn fetch_storage_batch() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();

	let keys =
		vec![hex::decode(SYSTEM_NUMBER_KEY).unwrap(), hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap()];
	let values = client.storage_batch(&keys, hash).await.unwrap();

	assert_eq!(values.len(), 2);
	// Both System::Number and System::ParentHash should exist
	assert!(values[0].is_some());
	assert!(values[1].is_some());
}

#[tokio::test]
async fn fetch_system_chain() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();

	let chain_name = client.system_chain().await.unwrap();

	// Chain should return a non-empty name
	assert!(!chain_name.is_empty());
}

#[tokio::test]
async fn fetch_system_properties() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();

	// Just verify the call succeeds - ink-node may not have all standard properties
	let _properties = client.system_properties().await.unwrap();
}

// =============================================================================
// Error path tests
// =============================================================================

#[tokio::test]
async fn connect_to_invalid_endpoint_fails() {
	// Use a port that's unlikely to have anything listening
	let endpoint: Url = "ws://127.0.0.1:19999".parse().unwrap();
	let result = ForkRpcClient::connect(&endpoint).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		matches!(err, RpcClientError::ConnectionFailed { .. }),
		"Expected ConnectionFailed, got: {err:?}"
	);
}

#[tokio::test]
async fn fetch_header_non_existent_block_fails() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();

	// Use a fabricated block hash that doesn't exist
	let non_existent_hash = H256::from([0xde; 32]);
	let result = client.header(non_existent_hash).await;

	assert!(result.is_err());
	let err = result.unwrap_err();
	assert!(
		matches!(err, RpcClientError::InvalidResponse(_)),
		"Expected InvalidResponse for non-existent block, got: {err:?}"
	);
}

#[tokio::test]
async fn fetch_storage_non_existent_key_returns_none() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();

	// Use a fabricated storage key that doesn't exist
	let non_existent_key = vec![0xff; 32];
	let result = client.storage(&non_existent_key, hash).await.unwrap();

	// Non-existent storage returns None, not an error
	assert!(result.is_none());
}

#[tokio::test]
async fn fetch_storage_batch_with_mixed_keys() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();

	// Mix of existing and non-existing keys
	let keys = vec![
		hex::decode(SYSTEM_NUMBER_KEY).unwrap(), // exists
		vec![0xff; 32],                          // doesn't exist
	];
	let values = client.storage_batch(&keys, hash).await.unwrap();

	assert_eq!(values.len(), 2);
	assert!(values[0].is_some(), "System::Number should exist");
	assert!(values[1].is_none(), "Fabricated key should not exist");
}

#[tokio::test]
async fn fetch_storage_batch_empty_keys() {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().unwrap();
	let client = ForkRpcClient::connect(&endpoint).await.unwrap();
	let hash = client.finalized_head().await.unwrap();

	// Empty keys should return empty results
	let values = client.storage_batch(&[], hash).await.unwrap();
	assert!(values.is_empty());
}
