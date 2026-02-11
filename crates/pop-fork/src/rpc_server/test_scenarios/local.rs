// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

//! Integration tests for local storage layer behavior against real chain state.

use crate::{
	LocalStorageLayer,
	testing::{
		TestContext,
		constants::{SYSTEM_NUMBER_KEY, SYSTEM_PALLET_PREFIX, SYSTEM_PARENT_HASH_KEY},
	},
};
use std::time::Duration;
use subxt::ext::codec::Decode;

/// Helper to create a LocalStorageLayer with proper block hash and number
fn create_layer(ctx: &TestContext) -> LocalStorageLayer {
	LocalStorageLayer::new(
		ctx.remote().clone(),
		ctx.block_number(),
		ctx.block_hash(),
		ctx.metadata().clone(),
	)
}

macro_rules! assert_value {
	($result:expr, $expected:expr) => {
		assert_eq!($result.as_ref().and_then(|v| v.value.as_deref()), Some($expected));
	};
}

// Tests for new()
pub async fn new_creates_empty_layer() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Verify empty modifications
	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 0, "New layer should have no modifications");
	assert_eq!(layer.get_current_block_number(), ctx.block_number() + 1);
}

// Tests for get()
pub async fn get_returns_local_modification() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"test_key";
	let value = b"test_value";

	// Set a local value
	layer.set(key, Some(value)).unwrap();

	// Get should return the local value
	let result = layer.get(block, key).await.unwrap();
	assert_value!(result, value.as_slice());

	// After a few commits, the last modification blocks remains the same
	layer.commit().await.unwrap();
	layer.commit().await.unwrap();
	let new_block = layer.get_current_block_number();
	let result = layer.get(new_block, key).await.unwrap();
	assert_value!(result, value.as_slice());
}

pub async fn get_non_existent_block_returns_none() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Query a block that doesn't exist
	let non_existent_block = u32::MAX;
	let key = b"some_key";

	let result = layer.get(non_existent_block, key).await.unwrap();
	assert!(result.is_none(), "Non-existent block should return None");
}

pub async fn get_returns_none_for_deleted_prefix_if_exact_key_not_found() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"key";
	let prefix = b"ke";
	let value = b"value";

	layer.set(key, Some(value)).unwrap();

	layer.delete_prefix(prefix).unwrap();

	// Get should return None
	let result = layer.get(block, key).await.unwrap();
	assert!(result.is_none());
}

pub async fn get_returns_some_for_deleted_prefix_if_exact_key_found_after_deletion() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"key";
	let prefix = b"ke";
	let value = b"value";

	layer.set(key, Some(value)).unwrap();

	layer.delete_prefix(prefix).unwrap();

	// Get should return None
	let result = layer.get(block, key).await.unwrap();
	assert!(result.is_none(), "get() should return None for deleted key");

	layer.set(key, Some(value)).unwrap();
	let result = layer.get(block, key).await.unwrap();
	// the exact key is found
	assert_eq!(result.unwrap().value.as_deref().unwrap(), value.as_slice());
	// even for a deleted prefix
	assert!(layer.is_deleted(prefix).unwrap());
}

pub async fn get_falls_back_to_parent() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

	// Get without local modification - should fetch from parent
	let result = layer.get(block, &key).await.unwrap().unwrap().value.clone().unwrap();
	assert_eq!(u32::decode(&mut &result[..]).unwrap(), ctx.block_number());
}

pub async fn get_local_overrides_parent() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let local_value = b"local_override";

	// Get parent value first
	let parent_value = layer.get(block, &key).await.unwrap().unwrap().value.clone().unwrap();
	assert_eq!(u32::decode(&mut &parent_value[..]).unwrap(), ctx.block_number());

	// Set local value
	layer.set(&key, Some(local_value)).unwrap();

	// Get should return local value, not parent
	let result = layer.get(block, &key).await.unwrap();
	assert_value!(result, local_value.as_slice());
}

pub async fn get_returns_none_for_nonexistent_key() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"nonexistent_key_12345";

	// Get should return None for nonexistent key
	let result = layer.get(block, key).await.unwrap();
	assert!(result.is_none());
}

pub async fn get_retrieves_modified_value_from_fork_history() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	let key = b"modified_key";
	let value_block_1 = b"value_at_block_1";
	let value_block_2 = b"value_at_block_2";

	// Advance one block to be fully inside the fork.
	layer.commit().await.unwrap();

	// Set and commit at block N (first_forked_block)
	layer.set(key, Some(value_block_1)).unwrap();
	layer.commit().await.unwrap();
	let block_1 = layer.get_current_block_number() - 1; // Block where we committed

	// Set and commit at block N+1
	layer.set(key, Some(value_block_2)).unwrap();
	layer.commit().await.unwrap();
	let block_2 = layer.get_current_block_number() - 1; // Block where we committed

	// Query at block_1 - should get value_block_1 from local_storage table
	let result_block_1 = layer.get(block_1, key).await.unwrap();
	assert_value!(result_block_1, value_block_1.as_slice());

	// Query at block_2 - should get value_block_2 from local_storage table
	let result_block_2 = layer.get(block_2, key).await.unwrap();
	assert_value!(result_block_2, value_block_2.as_slice());

	// Query at latest block - should get value_block_2 from modifications
	let result_latest = layer.get(layer.get_current_block_number(), key).await.unwrap();
	assert_value!(result_latest, value_block_2.as_slice());
}

pub async fn get_retrieves_unmodified_value_from_remote_at_past_forked_block() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	let unmodified_key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

	// Advance a few blocks
	layer.commit().await.unwrap();
	layer.commit().await.unwrap();
	let committed_block = layer.get_current_block_number() - 1;

	// Query the unmodified_key at the committed block
	// Since unmodified_key was never modified, it should fall back to remote at
	// first_forked_block
	let result = layer.get(committed_block, &unmodified_key).await.unwrap();
	assert!(result.is_some(),);

	// Verify we get the same value as querying at first_forked_block directly
	let remote_value = layer.get(ctx.block_number(), &unmodified_key).await.unwrap();
	assert_eq!(result, remote_value,);
}

// Tests for get_block (via get/get_batch for historical blocks)
pub async fn get_historical_block() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Query a block that's not in cache (fork point)
	let block_number = ctx.block_number();
	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

	// Verify block is not in cache initially
	let cached_before = ctx.remote().cache().get_block_by_number(block_number).await.unwrap();
	assert!(cached_before.is_none());

	// Get storage from historical block
	let result = layer.get(block_number, &key).await.unwrap().unwrap().value.clone().unwrap();
	assert_eq!(u32::decode(&mut &result[..]).unwrap(), ctx.block_number());

	// Cached after
	let cached_before = ctx.remote().cache().get_block_by_number(block_number).await.unwrap();
	assert!(cached_before.is_some());
}

// Tests for set()
pub async fn set_stores_value() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"key";
	let value = b"value";

	layer.set(key, Some(value)).unwrap();

	// Verify via get
	let result = layer.get(block, key).await.unwrap();
	assert_value!(result, value.as_slice());
}

pub async fn set_overwrites_previous_value() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"key";
	let value1 = b"value1";
	let value2 = b"value2";

	layer.set(key, Some(value1)).unwrap();
	layer.set(key, Some(value2)).unwrap();

	// Should have the second value
	let result = layer.get(block, key).await.unwrap();
	assert_eq!(result.as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
}

// Tests for get_batch()
pub async fn get_batch_empty_keys() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let results = layer.get_batch(ctx.block_number(), &[]).await.unwrap();
	assert_eq!(results.len(), 0);
}

pub async fn get_batch_returns_local_modifications() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key1 = b"key1";
	let key2 = b"key2";
	let value1 = b"value1";
	let value2 = b"value2";

	layer.set_batch(&[(key1, Some(value1)), (key2, Some(value2))]).unwrap();

	let results = layer.get_batch(block, &[key1, key2]).await.unwrap();
	assert_eq!(results.len(), 2);
	assert_eq!(results[0].as_ref().and_then(|v| v.value.as_deref()), Some(value1.as_slice()));
	assert_eq!(results[1].as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
}

pub async fn get_batch_returns_none_for_deleted_prefix() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key1 = b"key1";
	let key2 = b"key2";

	layer.set_batch(&[(key1, Some(b"val")), (key2, Some(b"val"))]).unwrap();
	layer.delete_prefix(key2).unwrap();

	let results = layer.get_batch(block, &[key1, key2]).await.unwrap();
	assert!(results[0].is_some());
	assert!(results[1].is_none());
}

pub async fn get_batch_falls_back_to_parent() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

	let results = layer.get_batch(block, &[key1.as_slice(), key2.as_slice()]).await.unwrap();
	assert!(results[0].is_some());
	assert!(results[1].is_some());
}

pub async fn get_batch_local_overrides_parent() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
	let local_value = b"local_override";

	// Set one key locally
	layer.set(&key1, Some(local_value)).unwrap();

	let results = layer.get_batch(block, &[key1.as_slice(), key2.as_slice()]).await.unwrap();
	assert_eq!(results[0].as_ref().and_then(|v| v.value.as_deref()), Some(local_value.as_slice()));
	assert!(results[1].is_some());
}

pub async fn get_batch_mixed_sources() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let local_key = b"local_key";
	let remote_key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let deleted_key = b"deleted_key";
	let nonexistent_key = b"nonexistent_key";

	layer.set(local_key, Some(b"local_value")).unwrap();
	layer.set(deleted_key, None).unwrap();

	let results = layer
		.get_batch(block, &[local_key, remote_key.as_slice(), deleted_key, nonexistent_key])
		.await
		.unwrap();

	assert_eq!(results.len(), 4);
	assert_eq!(
		results[0].as_ref().and_then(|v| v.value.as_deref()),
		Some(b"local_value".as_slice())
	);
	assert_eq!(
		u32::decode(&mut &results[1].as_ref().unwrap().value.as_ref().unwrap()[..]).unwrap(),
		ctx.block_number()
	); // from parent
	assert!(results[2].as_ref().map(|v| v.value.is_none()).unwrap_or(false)); // deleted (has SharedValue with value: None)
	assert!(results[3].is_none()); // nonexistent
}

pub async fn get_batch_maintains_order() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key1 = b"key1";
	let key2 = b"key2";
	let key3 = b"key3";
	let value1 = b"value1";
	let value2 = b"value2";
	let value3 = b"value3";

	layer
		.set_batch(&[(key1, Some(value1)), (key2, Some(value2)), (key3, Some(value3))])
		.unwrap();

	// Request in different order
	let results = layer.get_batch(block, &[key3, key1, key2]).await.unwrap();
	assert_eq!(results[0].as_ref().and_then(|v| v.value.as_deref()), Some(value3.as_slice()));
	assert_eq!(results[1].as_ref().and_then(|v| v.value.as_deref()), Some(value1.as_slice()));
	assert_eq!(results[2].as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
}

pub async fn get_batch_retrieves_modified_value_from_fork_history() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	let key1 = b"modified_key1";
	let key2 = b"modified_key2";
	let value1_block_1 = b"value1_at_block_1";
	let value2_block_1 = b"value2_at_block_1";
	let value1_block_2 = b"value1_at_block_2";
	let value2_block_2 = b"value2_at_block_2";

	// Advance one block to be fully inside the fork
	layer.commit().await.unwrap();

	// Set and commit at block N
	layer
		.set_batch(&[(key1, Some(value1_block_1)), (key2, Some(value2_block_1))])
		.unwrap();
	layer.commit().await.unwrap();
	let block_1 = layer.get_current_block_number() - 1;

	// Set and commit at block N+1
	layer
		.set_batch(&[(key1, Some(value1_block_2)), (key2, Some(value2_block_2))])
		.unwrap();
	layer.commit().await.unwrap();
	let block_2 = layer.get_current_block_number() - 1;

	// Query at block_1 - should get values from local_storage table
	let results_block_1 = layer.get_batch(block_1, &[key1, key2]).await.unwrap();
	assert_value!(results_block_1[0], value1_block_1.as_slice());
	assert_value!(results_block_1[1], value2_block_1.as_slice());

	// Query at block_2 - should get values from local_storage table
	let results_block_2 = layer.get_batch(block_2, &[key1, key2]).await.unwrap();
	assert_value!(results_block_2[0], value1_block_2.as_slice());
	assert_value!(results_block_2[1], value2_block_2.as_slice());

	// Query at latest block - should get values from modifications
	let results_latest =
		layer.get_batch(layer.get_current_block_number(), &[key1, key2]).await.unwrap();
	assert_value!(results_latest[0], value1_block_2.as_slice());
	assert_value!(results_latest[1], value2_block_2.as_slice());
}

pub async fn get_batch_retrieves_unmodified_value_from_remote_at_past_forked_block() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	let unmodified_key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let unmodified_key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

	// Advance a few blocks
	layer.commit().await.unwrap();
	layer.commit().await.unwrap();
	let committed_block = layer.get_current_block_number() - 1;

	// Query the unmodified keys at the committed block
	// Since they were never modified, they should fall back to remote at first_forked_block
	let results = layer
		.get_batch(committed_block, &[unmodified_key1.as_slice(), unmodified_key2.as_slice()])
		.await
		.unwrap();
	assert!(results[0].is_some());
	assert!(results[1].is_some());

	// Verify we get the same values as querying at first_forked_block directly
	let remote_values = layer
		.get_batch(ctx.block_number(), &[unmodified_key1.as_slice(), unmodified_key2.as_slice()])
		.await
		.unwrap();
	assert_eq!(results[0], remote_values[0]);
	assert_eq!(results[1], remote_values[1]);
}

pub async fn get_batch_historical_block() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Wait for some blocks to be finalized
	std::thread::sleep(Duration::from_secs(30));

	// Query a block that's not in cache
	let block_number = ctx.block_number();
	let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
	let key3 = b"non_existent_key";

	// Get storage from historical block
	let results = layer
		.get_batch(block_number, &[key1.as_slice(), key2.as_slice(), key3])
		.await
		.unwrap();
	assert_eq!(results.len(), 3);
	assert_eq!(
		u32::decode(&mut &results[0].as_ref().unwrap().value.as_ref().unwrap()[..]).unwrap(),
		block_number
	);
	assert!(results[2].is_none());
}

pub async fn get_batch_non_existent_block_returns_none() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Query a block that doesn't exist
	let non_existent_block = u32::MAX;
	let keys: Vec<&[u8]> = vec![b"key1", b"key2"];

	let results = layer.get_batch(non_existent_block, &keys).await.unwrap();
	assert_eq!(results.len(), 2);
	assert!(results[0].is_none(), "Non-existent block should return None");
	assert!(results[1].is_none(), "Non-existent block should return None");
}

pub async fn get_batch_mixed_block_scenarios() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	// Test multiple scenarios:
	// 1. Latest block (from modifications)
	// 2. Historical block (from cache/RPC)

	// Advance some blocks
	layer.commit().await.unwrap();
	layer.commit().await.unwrap();

	let latest_block_1 = layer.get_current_block_number();

	let key1 = b"local_key";
	let key2 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

	// Set a local modification
	layer.set(key1, Some(b"local_value")).unwrap();

	// Get from latest block (should hit modifications)
	let results1 = layer.get(latest_block_1, key1).await.unwrap();
	assert_eq!(results1.as_ref().and_then(|v| v.value.as_deref()), Some(b"local_value".as_slice()));

	// Get from historical block (should fetch and cache block)
	let historical_block = ctx.block_number();
	let results2 = layer
		.get(historical_block, key2.as_slice())
		.await
		.unwrap()
		.unwrap()
		.value
		.clone()
		.unwrap();
	assert_eq!(u32::decode(&mut &results2[..]).unwrap(), historical_block);

	// Commit block modifications
	layer.commit().await.unwrap();

	let latest_block_2 = layer.get_current_block_number();

	layer.set(key1, Some(b"local_value_2")).unwrap();

	let result_previous_block = layer.get(latest_block_1, key1).await.unwrap().unwrap();
	let result_latest_block = layer.get(latest_block_2, key1).await.unwrap().unwrap();

	assert_eq!(result_previous_block.value.as_deref(), Some(b"local_value".as_slice()));
	assert_eq!(result_latest_block.value.as_deref(), Some(b"local_value_2".as_slice()));
}

// Tests for set_batch()
pub async fn set_batch_empty_entries() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	layer.set_batch(&[]).unwrap();

	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 0);
}

pub async fn set_batch_stores_multiple_values() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let key1 = b"key1";
	let key2 = b"key2";
	let key3 = b"key3";
	let value1 = b"value1";
	let value2 = b"value2";
	let value3 = b"value3";

	layer
		.set_batch(&[(key1, Some(value1)), (key2, Some(value2)), (key3, Some(value3))])
		.unwrap();

	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 3);
}

pub async fn set_batch_with_deletions() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key1 = b"key1";
	let key2 = b"key2";
	let value1 = b"value1";

	layer.set_batch(&[(key1, Some(value1)), (key2, None)]).unwrap();

	let results = layer.get_batch(block, &[key1, key2]).await.unwrap();
	assert!(results[0].is_some());
	// Deleted keys return Some(SharedValue { value: None }) to distinguish from "not found"
	assert!(results[1].as_ref().map(|v| v.value.is_none()).unwrap_or(false));
}

pub async fn set_batch_overwrites_previous_values() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"key";
	let value1 = b"value1";
	let value2 = b"value2";

	layer.set(key, Some(value1)).unwrap();
	layer.set_batch(&[(key, Some(value2))]).unwrap();

	let result = layer.get(block, key).await.unwrap();
	assert_eq!(result.as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
}

pub async fn set_batch_duplicate_keys_last_wins() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"key";
	let value1 = b"value1";
	let value2 = b"value2";

	// Set same key twice in one batch - last should win
	layer.set_batch(&[(key, Some(value1)), (key, Some(value2))]).unwrap();

	let result = layer.get(block, key).await.unwrap();
	assert_eq!(result.as_ref().and_then(|v| v.value.as_deref()), Some(value2.as_slice()));
}

// Tests for delete_prefix()
pub async fn delete_prefix_removes_matching_keys() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = b"prefix_";
	let key1 = b"prefix_key1";
	let key2 = b"prefix_key2";
	let key3 = b"other_key";

	// Set values
	layer.set(key1, Some(b"val1")).unwrap();
	layer.set(key2, Some(b"val2")).unwrap();
	layer.set(key3, Some(b"val3")).unwrap();

	// Delete prefix
	layer.delete_prefix(prefix).unwrap();

	// Matching keys should be gone from modifications
	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 1, "Only non-matching key should remain");
	assert_eq!(diff[0].0, key3);
}

pub async fn delete_prefix_blocks_parent_reads() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();
	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();

	// Verify key exists in parent
	let before = layer.get(block, &key).await.unwrap();
	assert!(before.is_some());

	// Delete prefix
	layer.delete_prefix(&prefix).unwrap();

	// Should return None now
	let after = layer.get(block, &key).await.unwrap();
	assert!(after.is_none());
}

pub async fn delete_prefix_adds_to_deleted_prefixes() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = b"prefix_";

	layer.delete_prefix(prefix).unwrap();

	// Should be marked as deleted
	assert!(layer.is_deleted(prefix).unwrap());
}

pub async fn delete_prefix_with_empty_prefix() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let key1 = b"key1";
	let key2 = b"key2";

	layer.set(key1, Some(b"val1")).unwrap();
	layer.set(key2, Some(b"val2")).unwrap();

	// Delete empty prefix (matches everything)
	layer.delete_prefix(b"").unwrap();

	// All modifications should be removed
	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 0, "Empty prefix should delete all modifications");
}

// Tests for is_deleted()
pub async fn is_deleted_returns_false_initially() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = b"prefix_";

	assert!(!layer.is_deleted(prefix).unwrap());
}

pub async fn is_deleted_returns_true_after_delete() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = b"prefix_";

	layer.delete_prefix(prefix).unwrap();

	assert!(layer.is_deleted(prefix).unwrap());
}

pub async fn is_deleted_exact_match_only() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix1 = b"prefix_";
	let prefix2 = b"prefix_other";

	layer.delete_prefix(prefix1).unwrap();

	assert!(layer.is_deleted(prefix1).unwrap());
	assert!(!layer.is_deleted(prefix2).unwrap());
}

// Tests for diff()
pub async fn diff_returns_empty_initially() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 0);
}

pub async fn diff_returns_all_modifications() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let key1 = b"key1";
	let key2 = b"key2";
	let value1 = b"value1";
	let value2 = b"value2";

	layer.set(key1, Some(value1)).unwrap();
	layer.set(key2, Some(value2)).unwrap();

	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 2);
	assert!(diff.iter().any(|(k, v)| k == key1 &&
		v.as_ref().and_then(|v| v.value.as_deref()) == Some(value1.as_slice())));
	assert!(diff.iter().any(|(k, v)| k == key2 &&
		v.as_ref().and_then(|v| v.value.as_deref()) == Some(value2.as_slice())));
}

pub async fn diff_includes_deletions() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let key = b"deleted";

	layer.set(key, None).unwrap();

	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 1);
	assert_eq!(diff[0].0, key);
	// Deletion creates a SharedValue with value: None
	assert!(diff[0].1.as_ref().map(|v| v.value.is_none()).unwrap_or(false));
}

pub async fn diff_excludes_prefix_deleted_keys() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = b"prefix_";
	let key = b"prefix_key";

	layer.set(key, Some(b"value")).unwrap();
	layer.delete_prefix(prefix).unwrap();

	// Key should be removed from modifications
	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 0, "diff() should not include prefix-deleted keys");
}

// Tests for commit()
pub async fn commit_writes_to_cache() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	let block = layer.get_current_block_number();

	let key1 = b"commit_key1";
	let key2 = b"commit_key2";
	let value1 = b"commit_value1";
	let value2 = b"commit_value2";

	// Set local modifications
	layer.set(key1, Some(value1)).unwrap();
	layer.set(key2, Some(value2)).unwrap();

	// Verify not in cache yet
	assert!(
		ctx.remote()
			.cache()
			.get_local_value_at_block(key1, block)
			.await
			.unwrap()
			.is_none()
	);
	assert!(
		ctx.remote()
			.cache()
			.get_local_value_at_block(key2, block)
			.await
			.unwrap()
			.is_none()
	);

	// Commit
	layer.commit().await.unwrap();

	// Verify now in cache at the block_number it was committed to
	let cached1 = ctx.remote().cache().get_local_value_at_block(key1, block).await.unwrap();
	let cached2 = ctx.remote().cache().get_local_value_at_block(key2, block).await.unwrap();

	assert_eq!(cached1, Some(Some(value1.to_vec())));
	assert_eq!(cached2, Some(Some(value2.to_vec())));
}

pub async fn commit_preserves_modifications() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	let block = layer.get_current_block_number();

	let key = b"preserve_key";
	let value = b"preserve_value";

	// Set and commit
	layer.set(key, Some(value)).unwrap();
	layer.commit().await.unwrap();

	// Modifications should still be in local layer
	let local_result = layer.get(block + 1, key).await.unwrap();
	assert_eq!(local_result.as_ref().and_then(|v| v.value.as_deref()), Some(value.as_slice()));

	// Should also be in diff
	let diff = layer.diff().unwrap();
	assert_eq!(diff.len(), 1);
	assert_eq!(diff[0].0, key);
}

pub async fn commit_with_deletions() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key1 = b"delete_key1";
	let key2 = b"delete_key2";
	let value = b"value";

	// Set one value and mark another as deleted
	layer.set(key1, Some(value)).unwrap();
	layer.set(key2, None).unwrap();

	// Commit
	layer.commit().await.unwrap();

	// Both should be in cache
	let cached1 = ctx.remote().cache().get_local_value_at_block(key1, block).await.unwrap();
	let cached2 = ctx.remote().cache().get_local_value_at_block(key2, block).await.unwrap();

	assert_eq!(cached1, Some(Some(value.to_vec())));
	assert_eq!(cached2, Some(None)); // Cached as deletion (row exists with NULL value)
}

pub async fn commit_empty_modifications() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	// Commit with no modifications should succeed
	let result = layer.commit().await;
	assert!(result.is_ok());
}

pub async fn commit_multiple_times() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);
	let block = layer.get_current_block_number();

	let key = b"multi_block_key";
	let value = b"multi_block_value";

	// Set local modification
	layer.set(key, Some(value)).unwrap();

	// Commit multiple times - each commit increments the block number
	layer.commit().await.unwrap();
	layer.commit().await.unwrap();

	// Both block numbers should find the value in cache
	let cached1 = ctx.remote().cache().get_local_value_at_block(key, block).await.unwrap();
	let cached2 = ctx.remote().cache().get_local_value_at_block(key, block + 1).await.unwrap();

	assert_eq!(cached1, Some(Some(value.to_vec())));
	assert_eq!(cached2, Some(Some(value.to_vec())));
}

pub async fn commit_validity_ranges_work_properly() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	let key = b"validity_test_key";
	let value1 = b"value_version_1";
	let value2 = b"value_version_2";

	// Block N: Set initial value and commit
	let block_n = layer.get_current_block_number();
	layer.set(key, Some(value1)).unwrap();
	layer.commit().await.unwrap();

	// Verify key was created in local_keys
	let key_row = ctx.remote().cache().get_local_key(key).await.unwrap();
	assert!(key_row.is_some());
	let key_id = key_row.unwrap().id;

	// Verify value1 is valid from block_n onwards
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n).await.unwrap(),
		Some(Some(value1.to_vec()))
	);

	// Block N+1, N+2: Commit without changes (value should remain valid)
	layer.commit().await.unwrap();
	layer.commit().await.unwrap();

	// Value1 should still be valid at blocks N+1 and N+2
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n + 1).await.unwrap(),
		Some(Some(value1.to_vec()))
	);
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n + 2).await.unwrap(),
		Some(Some(value1.to_vec()))
	);

	// Block N+3: Update the value and commit
	layer.set(key, None).unwrap();
	layer.commit().await.unwrap();

	// Verify validity ranges:
	// - value1 should be valid from block_n to block_n_plus_3 (exclusive)
	// - value2 should be valid from block_n_plus_3 onwards
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n).await.unwrap(),
		Some(Some(value1.to_vec())),
	);
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n + 2).await.unwrap(),
		Some(Some(value1.to_vec())),
	);
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n + 3).await.unwrap(),
		Some(None),
	);
	assert_eq!(
		ctx.remote()
			.cache()
			.get_local_value_at_block(key, block_n + 3 + 10)
			.await
			.unwrap(),
		Some(None),
	);

	// Block N+4: Another update
	layer.set(key, Some(value2)).unwrap();
	layer.commit().await.unwrap();

	// Verify all three validity ranges
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n).await.unwrap(),
		Some(Some(value1.to_vec()))
	);
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n + 3).await.unwrap(),
		Some(None)
	);
	assert_eq!(
		ctx.remote().cache().get_local_value_at_block(key, block_n + 4).await.unwrap(),
		Some(Some(value2.to_vec()))
	);

	// Key ID should remain the same throughout
	let key_row_after = ctx.remote().cache().get_local_key(key).await.unwrap();
	assert_eq!(key_row_after.unwrap().id, key_id);
}

// Tests for next_key()
pub async fn next_key_returns_next_key_from_parent() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

	// Get the first key in the System pallet (starting from empty key)
	let first_key = layer.next_key(&prefix, &[]).await.unwrap();
	assert!(first_key.is_some(), "System pallet should have at least one key");

	let first_key = first_key.unwrap();
	assert!(first_key.starts_with(&prefix), "Returned key should start with the prefix");

	// Get the next key after the first one
	let second_key = layer.next_key(&prefix, &first_key).await.unwrap();
	assert!(second_key.is_some(), "System pallet should have more than one key");

	let second_key = second_key.unwrap();
	assert!(second_key.starts_with(&prefix), "Second key should also start with the prefix");
	assert!(second_key > first_key, "Second key should be greater than first key");
}

pub async fn next_key_returns_none_when_no_more_keys() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Use a prefix that doesn't exist
	let nonexistent_prefix = b"nonexistent_prefix_12345";

	let result = layer.next_key(nonexistent_prefix, &[]).await.unwrap();
	assert!(result.is_none(), "Should return None for nonexistent prefix");
}

pub async fn next_key_skips_deleted_prefix() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

	// Get the first two keys
	let first_key = layer.next_key(&prefix, &[]).await.unwrap().unwrap();
	let second_key = layer.next_key(&prefix, &first_key).await.unwrap().unwrap();

	// Delete the prefix that matches the first key (delete the first key specifically)
	layer.delete_prefix(&first_key).unwrap();

	// Now when we query from empty, we should skip the first key and get the second
	let result = layer.next_key(&prefix, &[]).await.unwrap();
	assert!(result.is_some(), "Should find a key after skipping deleted one");
	assert_eq!(
		result.unwrap(),
		second_key,
		"Should return second key after skipping deleted first key"
	);
}

pub async fn next_key_skips_multiple_deleted_keys() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

	// Get the first three keys
	let first_key = layer.next_key(&prefix, &[]).await.unwrap().unwrap();
	let second_key = layer.next_key(&prefix, &first_key).await.unwrap().unwrap();
	let third_key = layer.next_key(&prefix, &second_key).await.unwrap().unwrap();

	// Delete the first two keys
	layer.delete_prefix(&first_key).unwrap();
	layer.delete_prefix(&second_key).unwrap();

	// Query from empty should skip both and return the third
	let result = layer.next_key(&prefix, &[]).await.unwrap();
	assert!(result.is_some(), "Should find a key after skipping deleted ones");
	assert_eq!(
		result.unwrap(),
		third_key,
		"Should return third key after skipping first two deleted keys"
	);
}

pub async fn next_key_returns_none_when_all_remaining_deleted() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();

	// Delete the entire System pallet prefix
	layer.delete_prefix(&prefix).unwrap();

	// All keys under System pallet should be skipped
	let result = layer.next_key(&prefix, &[]).await.unwrap();
	assert!(result.is_none(), "Should return None when all keys match deleted prefix");
}

pub async fn next_key_with_empty_prefix() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Empty prefix should match all keys
	let result = layer.next_key(&[], &[]).await.unwrap();
	assert!(result.is_some(), "Empty prefix should return some key from storage");
}

pub async fn next_key_with_nonexistent_prefix() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	let nonexistent_prefix = b"this_prefix_definitely_does_not_exist_xyz";

	let result = layer.next_key(nonexistent_prefix, &[]).await.unwrap();
	assert!(result.is_none(), "Nonexistent prefix should return None");
}

pub async fn metadata_at_returns_metadata_for_future_blocks() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Metadata registered at fork point should be valid for future blocks too
	let future_block = ctx.block_number() + 100;
	let metadata = layer.metadata_at(future_block).await.unwrap();
	assert!(metadata.pallets().count() > 0, "Metadata should be valid for future blocks");
}

pub async fn metadata_at_fetches_from_remote_for_pre_fork_blocks() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// For blocks before fork point, metadata should be fetched from remote
	if ctx.block_number() > 0 {
		let metadata = layer.metadata_at(ctx.block_number() - 1).await.unwrap();
		assert!(metadata.pallets().count() > 0, "Should fetch metadata from remote");
	}
}

pub async fn register_metadata_version_adds_new_version() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Register a new metadata version at a future block
	let new_block = ctx.block_number() + 10;
	layer.register_metadata_version(new_block, ctx.metadata().clone()).unwrap();

	// Both versions should be accessible
	let old_metadata = layer.metadata_at(ctx.block_number()).await.unwrap();
	let new_metadata = layer.metadata_at(new_block).await.unwrap();

	assert!(old_metadata.pallets().count() > 0);
	assert!(new_metadata.pallets().count() > 0);
}

pub async fn register_metadata_version_respects_block_boundaries() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Register new metadata at block X+5
	let upgrade_block = ctx.block_number() + 5;
	layer.register_metadata_version(upgrade_block, ctx.metadata().clone()).unwrap();

	// Blocks before upgrade should get original metadata
	// Blocks at or after upgrade should get new metadata
	let before_upgrade = layer.metadata_at(upgrade_block - 1).await.unwrap();
	let at_upgrade = layer.metadata_at(upgrade_block).await.unwrap();
	let after_upgrade = layer.metadata_at(upgrade_block + 10).await.unwrap();

	// All should have pallets (same metadata in this test, but different Arc instances
	// after upgrade point)
	assert!(before_upgrade.pallets().count() > 0);
	assert!(at_upgrade.pallets().count() > 0);
	assert!(after_upgrade.pallets().count() > 0);
}

pub async fn has_code_changed_at_returns_false_when_no_code_modified() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// No modifications made, should return false
	let result = layer.has_code_changed_at(ctx.block_number()).unwrap();
	assert!(!result, "Should return false when no code was modified");
}

pub async fn has_code_changed_at_returns_false_for_non_code_modifications() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Modify a non-code key
	layer.set(b"some_random_key", Some(b"some_value")).unwrap();

	let block = layer.get_current_block_number();
	let result = layer.has_code_changed_at(block).unwrap();
	assert!(!result, "Should return false when only non-code keys modified");
}

pub async fn has_code_changed_at_returns_true_when_code_modified() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Modify the :code key
	let code_key = sp_core::storage::well_known_keys::CODE;
	layer.set(code_key, Some(b"new_runtime_code")).unwrap();

	let block = layer.get_current_block_number();
	let result = layer.has_code_changed_at(block).unwrap();
	assert!(result, "Should return true when :code was modified at the specified block");
}

pub async fn has_code_changed_at_returns_false_for_different_block() {
	let ctx = TestContext::for_local().await;
	let layer = create_layer(&ctx);

	// Modify the :code key at current block
	let code_key = sp_core::storage::well_known_keys::CODE;
	layer.set(code_key, Some(b"new_runtime_code")).unwrap();

	let current_block = layer.get_current_block_number();

	// Check a different block number - should return false
	let result = layer.has_code_changed_at(current_block + 1).unwrap();
	assert!(!result, "Should return false when checking different block than modification");

	let result = layer.has_code_changed_at(current_block - 1).unwrap();
	assert!(!result, "Should return false when checking block before modification");
}

pub async fn has_code_changed_at_tracks_modification_block_correctly() {
	let ctx = TestContext::for_local().await;
	let mut layer = create_layer(&ctx);

	let code_key = sp_core::storage::well_known_keys::CODE;
	let first_block = layer.get_current_block_number();

	// Modify code at first block
	layer.set(code_key, Some(b"runtime_v1")).unwrap();
	assert!(
		layer.has_code_changed_at(first_block).unwrap(),
		"Code should be marked as changed at first block"
	);

	// Commit and advance to next block
	layer.commit().await.unwrap();
	let second_block = layer.get_current_block_number();

	// Code was modified at first_block, not second_block
	assert!(
		layer.has_code_changed_at(first_block).unwrap(),
		"Code change should still be recorded at first block"
	);
	assert!(
		!layer.has_code_changed_at(second_block).unwrap(),
		"Code should not be marked as changed at second block (no new modification)"
	);

	// Modify code again at second block
	layer.set(code_key, Some(b"runtime_v2")).unwrap();
	assert!(
		layer.has_code_changed_at(second_block).unwrap(),
		"Code should be marked as changed at second block after new modification"
	);
}
