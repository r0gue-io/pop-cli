// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

//! Integration tests for block lifecycle behavior against a real node.

use crate::{Block, BlockError, testing::TestContext};
use subxt::config::substrate::H256;

pub async fn fork_point_with_hash_creates_block_with_correct_metadata() {
	let ctx = TestContext::for_storage().await;

	let expected_parent_hash = ctx.rpc().header(ctx.block_hash()).await.unwrap().parent_hash;

	let block = Block::fork_point(&ctx.endpoint, ctx.cache().clone(), ctx.block_hash().into())
		.await
		.unwrap();

	assert_eq!(block.number, ctx.block_number());
	assert_eq!(block.hash, ctx.block_hash());
	assert_eq!(block.parent_hash, expected_parent_hash);
	assert!(!block.header.is_empty());
	// Note: extrinsics may or may not be empty depending on the block
	assert!(block.parent.is_none());
}

pub async fn fork_point_with_non_existent_hash_returns_error() {
	let ctx = TestContext::for_storage().await;
	let non_existent_hash = H256::from([0xde; 32]);

	let result =
		Block::fork_point(&ctx.endpoint, ctx.cache().clone(), non_existent_hash.into()).await;

	assert!(matches!(result, Err(BlockError::BlockHashNotFound(h)) if h == non_existent_hash));
}

pub async fn fork_point_with_number_creates_block_with_correct_metadata() {
	let ctx = TestContext::for_storage().await;
	let expected_parent_hash = ctx.rpc().header(ctx.block_hash()).await.unwrap().parent_hash;

	let block = Block::fork_point(&ctx.endpoint, ctx.cache().clone(), ctx.block_number().into())
		.await
		.unwrap();

	assert_eq!(block.number, ctx.block_number());
	assert_eq!(block.hash, ctx.block_hash());
	assert_eq!(block.parent_hash, expected_parent_hash);
	assert!(!block.header.is_empty());
	// Note: extrinsics may or may not be empty depending on the block
	assert!(block.parent.is_none());
}

pub async fn fork_point_with_non_existent_number_returns_error() {
	let ctx = TestContext::for_storage().await;
	let non_existent_number = u32::MAX;

	let result =
		Block::fork_point(&ctx.endpoint, ctx.cache().clone(), non_existent_number.into()).await;

	assert!(matches!(result, Err(BlockError::BlockNumberNotFound(n)) if n == non_existent_number));
}

pub async fn child_creates_block_with_correct_metadata() {
	let ctx = TestContext::for_storage().await;
	let mut parent = Block::fork_point(&ctx.endpoint, ctx.cache().clone(), ctx.block_hash().into())
		.await
		.unwrap();

	let child_hash = H256::from([0x42; 32]);
	let child_header = vec![1, 2, 3, 4];
	let child_extrinsics = vec![vec![5, 6, 7]];

	let child = parent
		.child(child_hash, child_header.clone(), child_extrinsics.clone())
		.await
		.unwrap();

	assert_eq!(child.number, parent.number + 1);
	assert_eq!(child.hash, child_hash);
	assert_eq!(child.parent_hash, parent.hash);
	assert_eq!(child.header, child_header);
	assert_eq!(child.extrinsics, child_extrinsics);
	assert_eq!(child.parent.unwrap().number, parent.number);
}

async fn get_storage_value(block: Block, number: u32, key: &[u8]) -> Vec<u8> {
	block
		.storage()
		.get(number, key)
		.await
		.unwrap()
		.as_deref()
		.unwrap()
		.value
		.clone()
		.unwrap()
}

pub async fn child_commits_parent_storage() {
	let ctx = TestContext::for_storage().await;
	let mut parent = Block::fork_point(&ctx.endpoint, ctx.cache().clone(), ctx.block_hash().into())
		.await
		.unwrap();

	let key = b"committed_key";
	let value = b"committed_value";

	// Set value on parent
	parent.storage_mut().set(key, Some(value)).unwrap();

	// Create child (this commits parent storage)
	let mut child = parent.child(H256::from([0x42; 32]), vec![], vec![]).await.unwrap();

	let value2 = b"committed_value2";
	child.storage_mut().set(key, Some(value2)).unwrap();

	// child.number is the latest committed block, but these changes aren't committed yet,
	// they're happening in the block we're building
	assert_eq!(get_storage_value(child.clone(), child.number + 1, key).await, value2);
	// child.number is the latest committed block
	assert_eq!(get_storage_value(child.clone(), child.number, key).await, value);
}

pub async fn child_storage_inherits_parent_modifications() {
	let ctx = TestContext::for_storage().await;
	let mut parent = Block::fork_point(&ctx.endpoint, ctx.cache().clone(), ctx.block_hash().into())
		.await
		.unwrap();

	let key = b"inherited_key";
	let value = b"inherited_value";

	parent.storage_mut().set(key, Some(value)).unwrap();

	let child = parent.child(H256::from([0x42; 32]), vec![], vec![]).await.unwrap();

	// Child should see the value at its block number
	assert_eq!(get_storage_value(child.clone(), child.number, key).await, value);
}
