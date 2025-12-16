// SPDX-License-Identifier: GPL-3.0

//! Integration tests for RemoteStorageLayer.
//!
//! These tests require a live RPC endpoint and are gated behind the `integration-tests` feature.
//!
//! Run with: `cargo nextest run -p pop-fork --features integration-tests --test remote`

#![cfg(feature = "integration-tests")]

use pop_fork::{ForkRpcClient, RemoteStorageLayer, StorageCache};
use url::Url;

/// Paseo testnet public RPC endpoint.
const PASEO_ENDPOINT: &str = "wss://rpc.ibp.network/paseo";

// Well-known storage keys for testing.
// These are derived from twox128 hashes of pallet and storage item names.

/// System::Number storage key: twox128("System") ++ twox128("Number")
const SYSTEM_NUMBER_KEY: &str = "26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac";

/// System::ParentHash storage key: twox128("System") ++ twox128("ParentHash")
const SYSTEM_PARENT_HASH_KEY: &str =
	"26aa394eea5630e07c48ae0c9558cef734abf5cb34d6244378cddbf18e849d96";

/// System pallet prefix: twox128("System")
const SYSTEM_PALLET_PREFIX: &str = "26aa394eea5630e07c48ae0c9558cef7";

async fn create_test_layer() -> RemoteStorageLayer {
	let endpoint: Url = PASEO_ENDPOINT.parse().unwrap();
	let rpc = ForkRpcClient::connect(&endpoint).await.unwrap();
	let cache = StorageCache::in_memory().await.unwrap();
	let block_hash = rpc.finalized_head().await.unwrap();

	RemoteStorageLayer::new(rpc, cache, block_hash)
}

#[tokio::test(flavor = "multi_thread")]
async fn get_fetches_and_caches() {
	let layer = create_test_layer().await;

	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

	// First call should fetch from RPC and cache
	let value1 = layer.get(&key).await.unwrap();
	assert!(value1.is_some(), "System::Number should exist");

	// Verify it was cached
	let cached = layer.cache().get_storage(layer.block_hash(), &key).await.unwrap();
	assert!(cached.is_some(), "Value should be cached after first get");
	assert_eq!(cached.unwrap(), value1);

	// Second call should return cached value (same result)
	let value2 = layer.get(&key).await.unwrap();
	assert_eq!(value1, value2);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_caches_empty_values() {
	let layer = create_test_layer().await;

	// Use a key that definitely doesn't exist
	let nonexistent_key = b"this_key_definitely_does_not_exist_12345";

	// First call fetches from RPC - should be None
	let value = layer.get(nonexistent_key).await.unwrap();
	assert!(value.is_none(), "Nonexistent key should return None");

	// Verify it was cached as empty (Some(None))
	let cached = layer.cache().get_storage(layer.block_hash(), nonexistent_key).await.unwrap();
	assert_eq!(cached, Some(None), "Empty value should be cached as Some(None)");
}

#[tokio::test(flavor = "multi_thread")]
async fn get_batch_fetches_mixed() {
	let layer = create_test_layer().await;

	let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
	let key3 = b"nonexistent_key".to_vec();

	let keys: Vec<&[u8]> = vec![key1.as_slice(), key2.as_slice(), key3.as_slice()];

	let results = layer.get_batch(&keys).await.unwrap();

	assert_eq!(results.len(), 3);
	assert!(results[0].is_some(), "System::Number should exist");
	assert!(results[1].is_some(), "System::ParentHash should exist");
	assert!(results[2].is_none(), "Nonexistent key should be None");

	// Verify all were cached
	for (i, key) in keys.iter().enumerate() {
		let cached = layer.cache().get_storage(layer.block_hash(), key).await.unwrap();
		assert!(cached.is_some(), "Key {} should be cached", i);
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn get_batch_uses_cache() {
	let layer = create_test_layer().await;

	let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

	// Pre-cache key1
	let value1 = layer.get(&key1).await.unwrap();

	// Batch get with one cached and one uncached
	let keys: Vec<&[u8]> = vec![key1.as_slice(), key2.as_slice()];
	let results = layer.get_batch(&keys).await.unwrap();

	assert_eq!(results.len(), 2);
	assert_eq!(results[0], value1, "Cached value should match");
	assert!(results[1].is_some(), "Uncached value should be fetched");
}

#[tokio::test(flavor = "multi_thread")]
async fn prefetch_prefix() {
	let layer = create_test_layer().await;

	let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

	// Prefetch all System storage items (page_size is the batch size per RPC call)
	let count = layer.prefetch_prefix(&prefix, 5).await.unwrap();

	assert!(count > 0, "Should have prefetched some keys");

	// Verify some values were cached
	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let cached = layer.cache().get_storage(layer.block_hash(), &key).await.unwrap();
	assert!(cached.is_some(), "Prefetched key should be cached");
}

#[tokio::test(flavor = "multi_thread")]
async fn layer_is_cloneable() {
	let layer = create_test_layer().await;

	// Clone the layer
	let layer2 = layer.clone();

	// Both should work and share the same cache
	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

	let value1 = layer.get(&key).await.unwrap();
	let value2 = layer2.get(&key).await.unwrap();

	assert_eq!(value1, value2);
}

#[tokio::test(flavor = "multi_thread")]
async fn accessor_methods() {
	let layer = create_test_layer().await;

	// Test accessor methods
	assert!(!layer.block_hash().is_zero());
	assert_eq!(layer.rpc().endpoint().as_str(), PASEO_ENDPOINT);
}
