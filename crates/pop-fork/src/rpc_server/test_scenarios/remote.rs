// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

//! Integration tests for remote storage and cache behavior.

use crate::testing::{
	TestContext,
	constants::{SYSTEM_NUMBER_KEY, SYSTEM_PALLET_PREFIX, SYSTEM_PARENT_HASH_KEY},
};
use std::time::Duration;

pub async fn get_fetches_and_caches() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();
	let block_hash = ctx.block_hash();

	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let value1 = layer.get(block_hash, &key).await.unwrap();
	assert!(value1.is_some(), "System::Number should exist");

	let cached = layer.cache().get_storage(block_hash, &key).await.unwrap();
	assert!(cached.is_some(), "Value should be cached after first get");
	assert_eq!(cached.unwrap(), value1);

	let value2 = layer.get(block_hash, &key).await.unwrap();
	assert_eq!(value1, value2);
}

pub async fn get_caches_empty_values() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();
	let block_hash = ctx.block_hash();

	let nonexistent_key = b"this_key_definitely_does_not_exist_12345";
	let value = layer.get(block_hash, nonexistent_key).await.unwrap();
	assert!(value.is_none(), "Nonexistent key should return None");

	let cached = layer.cache().get_storage(block_hash, nonexistent_key).await.unwrap();
	assert_eq!(cached, Some(None), "Empty value should be cached as Some(None)");
}

pub async fn get_batch_fetches_mixed() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();
	let block_hash = ctx.block_hash();

	let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();
	let key3 = b"nonexistent_key".to_vec();

	let keys: Vec<&[u8]> = vec![key1.as_slice(), key2.as_slice(), key3.as_slice()];
	let results = layer.get_batch(block_hash, &keys).await.unwrap();

	assert_eq!(results.len(), 3);
	assert!(results[0].is_some(), "System::Number should exist");
	assert!(results[1].is_some(), "System::ParentHash should exist");
	assert!(results[2].is_none(), "Nonexistent key should be None");

	for (i, key) in keys.iter().enumerate() {
		let cached = layer.cache().get_storage(block_hash, key).await.unwrap();
		assert!(cached.is_some(), "Key {} should be cached", i);
	}
}

pub async fn get_batch_uses_cache() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();
	let block_hash = ctx.block_hash();

	let key1 = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let key2 = hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap();

	let value1 = layer.get(block_hash, &key1).await.unwrap();
	let keys: Vec<&[u8]> = vec![key1.as_slice(), key2.as_slice()];
	let results = layer.get_batch(block_hash, &keys).await.unwrap();

	assert_eq!(results.len(), 2);
	assert_eq!(results[0], value1, "Cached value should match");
	assert!(results[1].is_some(), "Uncached value should be fetched");
}

pub async fn prefetch_prefix() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();
	let block_hash = ctx.block_hash();

	let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();
	let count = layer.prefetch_prefix(block_hash, &prefix, 5).await.unwrap();
	assert!(count > 0, "Should have prefetched some keys");

	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let cached = layer.cache().get_storage(block_hash, &key).await.unwrap();
	assert!(cached.is_some(), "Prefetched key should be cached");
}

pub async fn layer_is_cloneable() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();
	let block_hash = ctx.block_hash();
	let layer2 = layer.clone();

	let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
	let value1 = layer.get(block_hash, &key).await.unwrap();
	let value2 = layer2.get(block_hash, &key).await.unwrap();
	assert_eq!(value1, value2);
}

pub async fn accessor_methods() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();
	let block_hash = ctx.block_hash();

	assert!(!block_hash.is_zero());
	assert!(layer.rpc().endpoint().as_str().starts_with("ws://"));
}

pub async fn fetch_and_cache_block_by_number_caches_block() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();

	let finalized_hash = layer.rpc().finalized_head().await.unwrap();
	let finalized_header = layer.rpc().header(finalized_hash).await.unwrap();
	let finalized_number = finalized_header.number;

	let cached = layer.cache().get_block_by_number(finalized_number).await.unwrap();
	assert!(cached.is_none());

	let result = layer.fetch_and_cache_block_by_number(finalized_number).await.unwrap();
	assert!(result.is_some());

	let block_row = result.unwrap();
	assert_eq!(block_row.number, finalized_number as i64);
	assert_eq!(block_row.hash.len(), 32);
	assert_eq!(block_row.parent_hash.len(), 32);
	assert!(!block_row.header.is_empty());

	let cached = layer.cache().get_block_by_number(finalized_number).await.unwrap();
	assert!(cached.is_some());
	let cached_block = cached.unwrap();
	assert_eq!(cached_block.number, block_row.number);
	assert_eq!(cached_block.hash, block_row.hash);
	assert_eq!(cached_block.parent_hash, block_row.parent_hash);
	assert_eq!(cached_block.header, block_row.header);
}

pub async fn fetch_and_cache_block_by_number_non_existent() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();

	let non_existent_number = u32::MAX;
	let result = layer.fetch_and_cache_block_by_number(non_existent_number).await.unwrap();
	assert!(result.is_none(), "Non-existent block should return None");

	let cached = layer.cache().get_block_by_number(non_existent_number).await.unwrap();
	assert!(cached.is_none(), "Non-existent block should not be cached");
}

pub async fn fetch_and_cache_block_by_number_multiple_blocks() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();

	std::thread::sleep(Duration::from_secs(30));

	let finalized_hash = layer.rpc().finalized_head().await.unwrap();
	let finalized_header = layer.rpc().header(finalized_hash).await.unwrap();
	let finalized_number = finalized_header.number;

	let max_blocks = finalized_number.min(3);
	for block_num in 0..=max_blocks {
		let result = layer.fetch_and_cache_block_by_number(block_num).await.unwrap().unwrap();
		assert_eq!(result.number, block_num as i64);

		let cached = layer.cache().get_block_by_number(block_num).await.unwrap().unwrap();
		assert_eq!(cached.number, result.number);
		assert_eq!(cached.hash, result.hash);
	}
}

pub async fn fetch_and_cache_block_by_number_idempotent() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();

	let block_number = 0u32;
	let result1 = layer.fetch_and_cache_block_by_number(block_number).await.unwrap().unwrap();
	let result2 = layer.fetch_and_cache_block_by_number(block_number).await.unwrap().unwrap();

	assert_eq!(result1.number, result2.number);
	assert_eq!(result1.hash, result2.hash);
	assert_eq!(result1.parent_hash, result2.parent_hash);
	assert_eq!(result1.header, result2.header);
}

pub async fn fetch_and_cache_block_by_number_verifies_parent_chain() {
	let ctx = TestContext::for_remote().await;
	let layer = ctx.remote();

	std::thread::sleep(Duration::from_secs(30));

	let finalized_hash = layer.rpc().finalized_head().await.unwrap();
	let finalized_header = layer.rpc().header(finalized_hash).await.unwrap();
	let finalized_number = finalized_header.number;

	let max_blocks = finalized_number.min(3);
	let mut previous_hash: Option<Vec<u8>> = None;

	for block_num in 0..=max_blocks {
		let block_row = layer.fetch_and_cache_block_by_number(block_num).await.unwrap().unwrap();
		if let Some(prev_hash) = previous_hash {
			assert_eq!(
				block_row.parent_hash,
				prev_hash,
				"Block {} parent hash should match block {} hash",
				block_num,
				block_num - 1
			);
		}
		previous_hash = Some(block_row.hash.clone());
	}
}
