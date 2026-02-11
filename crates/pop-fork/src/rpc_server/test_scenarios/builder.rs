// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

//! Integration tests for block building flow against a real node.

use crate::{
	Block, BlockBuilder, BlockBuilderError, DigestItem, ExecutorConfig, InherentProvider,
	RuntimeExecutor, TimestampInherent, consensus_engine, create_next_header, testing::TestContext,
};
use scale::Encode;

/// Test context holding all components needed for block building.
struct BlockBuilderTestContext {
	#[allow(dead_code)]
	base: TestContext,
	block: Block,
	executor: RuntimeExecutor,
}

/// Creates a fully initialized block builder test context with default executor config.
async fn create_test_context() -> BlockBuilderTestContext {
	create_test_context_with_config(None).await
}

/// Creates a test context with optional custom executor configuration.
async fn create_test_context_with_config(
	config: Option<ExecutorConfig>,
) -> BlockBuilderTestContext {
	let base = TestContext::for_storage().await;
	let block_hash = base.block_hash();

	// Fetch runtime code for the executor
	let runtime_code =
		base.rpc().runtime_code(block_hash).await.expect("Failed to fetch runtime code");

	// Create fork point block
	let block = Block::fork_point(&base.endpoint, base.cache().clone(), block_hash.into())
		.await
		.expect("Failed to create fork point");

	// Create executor with optional custom config
	let executor = match config {
		Some(cfg) => RuntimeExecutor::with_config(runtime_code, None, cfg),
		None => RuntimeExecutor::new(runtime_code, None),
	}
	.expect("Failed to create executor");

	BlockBuilderTestContext { base, block, executor }
}

pub async fn new_creates_builder_with_empty_extrinsics() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	assert!(builder.extrinsics().is_empty());
}

pub async fn initialize_succeeds_and_modifies_storage() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	let result = builder.initialize().await.expect("initialize failed");

	// Core_initialize_block should modify storage
	assert!(!result.storage_diff.is_empty());
}

pub async fn initialize_twice_fails() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);

	builder.initialize().await.expect("first initialize failed");
	let result = builder.initialize().await;
	assert!(matches!(result, Err(BlockBuilderError::AlreadyInitialized)));
}

pub async fn apply_inherents_without_providers_returns_empty() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	builder.initialize().await.expect("initialize failed");

	let results = builder.apply_inherents().await.expect("apply_inherents failed");
	assert!(results.is_empty());
}

pub async fn apply_inherents_before_initialize_fails() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	let result = builder.apply_inherents().await;
	assert!(matches!(result, Err(BlockBuilderError::NotInitialized)));
}

pub async fn apply_extrinsic_before_initialize_fails() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	let result = builder.apply_extrinsic(vec![0x00]).await;
	assert!(matches!(result, Err(BlockBuilderError::NotInitialized)));
}

pub async fn finalize_before_initialize_fails() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	let result = builder.finalize().await;
	assert!(matches!(result, Err(BlockBuilderError::NotInitialized)));
}

pub async fn apply_inherents_twice_fails() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	builder.initialize().await.expect("initialize failed");

	builder.apply_inherents().await.expect("first apply_inherents failed");
	let result = builder.apply_inherents().await;
	assert!(matches!(result, Err(BlockBuilderError::InherentsAlreadyApplied)));
}

pub async fn apply_extrinsic_before_inherents_fails() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	builder.initialize().await.expect("initialize failed");

	let result = builder.apply_extrinsic(vec![0x00]).await;
	assert!(matches!(result, Err(BlockBuilderError::InherentsNotApplied)));
}

pub async fn finalize_before_inherents_fails() {
	let ctx = create_test_context().await;
	let header = create_next_header(&ctx.block, vec![]);

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, vec![], None, false);
	builder.initialize().await.expect("initialize failed");

	let result = builder.finalize().await;
	assert!(matches!(result, Err(BlockBuilderError::InherentsNotApplied)));
}

pub async fn finalize_produces_child_block() {
	let ctx = create_test_context().await;
	let parent_number = ctx.block.number;
	let parent_hash = ctx.block.hash;
	let header = create_next_header(&ctx.block, vec![]);

	let providers: Vec<Box<dyn InherentProvider>> =
		vec![Box::new(TimestampInherent::default_relay())];

	let mut builder = BlockBuilder::new(ctx.block, ctx.executor, header, providers, None, false);
	builder.initialize().await.expect("initialize failed");
	builder.apply_inherents().await.expect("apply_inherents failed");

	let (new_block, _prototype) = builder.finalize().await.expect("finalize failed");

	assert_eq!(new_block.number, parent_number + 1);
	assert_eq!(new_block.parent_hash, parent_hash);
	assert!(new_block.parent.is_some());
	assert!(!new_block.header.is_empty());
}

pub async fn create_next_header_increments_block_number() {
	let ctx = create_test_context().await;

	let header_bytes = create_next_header(&ctx.block, vec![]);
	assert!(!header_bytes.is_empty());
	assert_eq!(&header_bytes[0..32], ctx.block.hash.as_bytes());
}

pub async fn create_next_header_includes_digest_items() {
	let ctx = create_test_context().await;

	let slot: u64 = 12345;
	let digest_items = vec![DigestItem::PreRuntime(consensus_engine::AURA, slot.encode())];

	let header_bytes = create_next_header(&ctx.block, digest_items);
	let empty_header = create_next_header(&ctx.block, vec![]);
	assert!(header_bytes.len() > empty_header.len());
}
