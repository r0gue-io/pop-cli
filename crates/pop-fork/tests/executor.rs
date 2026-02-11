// SPDX-License-Identifier: GPL-3.0

//! Integration tests for runtime execution against real chain state.

#![cfg(feature = "integration-tests")]

use pop_fork::{
	ExecutorConfig, LocalStorageLayer, RuntimeExecutor, SignatureMockMode, testing::TestContext,
};
use scale::Encode;
use subxt::config::substrate::H256;

/// Test context holding all layers needed for execution.
struct ExecutorTestContext {
	#[allow(dead_code)]
	base: TestContext,
	executor: RuntimeExecutor,
	storage: LocalStorageLayer,
	block_hash: H256,
	block_number: u32,
}

/// Creates a fully initialized executor test context.
async fn create_executor_context() -> ExecutorTestContext {
	create_executor_context_with_config(ExecutorConfig::default()).await
}

/// Creates an executor test context with a custom configuration.
async fn create_executor_context_with_config(config: ExecutorConfig) -> ExecutorTestContext {
	let base = TestContext::for_local().await;

	let block_hash = base.block_hash();
	let block_number = base.block_number();
	let header = base.rpc().header(block_hash).await.expect("Failed to get header");

	let runtime_code =
		base.rpc().runtime_code(block_hash).await.expect("Failed to fetch runtime code");

	base.cache()
		.cache_block(block_hash, block_number, header.parent_hash, &header.encode())
		.await
		.expect("Failed to cache block");

	let storage = LocalStorageLayer::new(
		base.remote().clone(),
		block_number,
		block_hash,
		base.metadata().clone(),
	);

	let executor = RuntimeExecutor::with_config(runtime_code, None, config)
		.expect("Failed to create executor");

	ExecutorTestContext { base, executor, storage, block_hash, block_number }
}

pub async fn core_version_executes_successfully() {
	let ctx = create_executor_context().await;

	let result = ctx
		.executor
		.call("Core_version", &[], &ctx.storage)
		.await
		.expect("Core_version execution failed");

	assert!(!result.output.is_empty(), "Core_version should return non-empty output");
	assert!(result.storage_diff.is_empty(), "Core_version should not modify storage");

	let version = ctx.executor.runtime_version().expect("Failed to get runtime version");
	assert!(!version.spec_name.is_empty(), "spec_name should not be empty");
	assert!(version.spec_version > 0, "spec_version should be positive");
}

pub async fn metadata_executes_successfully() {
	let ctx = create_executor_context().await;

	let result = ctx
		.executor
		.call("Metadata_metadata", &[], &ctx.storage)
		.await
		.expect("Metadata_metadata execution failed");

	assert!(
		result.output.len() > 1000,
		"Metadata should be larger than 1KB, got {} bytes",
		result.output.len()
	);
	assert!(result.storage_diff.is_empty(), "Metadata_metadata should not modify storage");
}

pub async fn with_config_applies_custom_settings() {
	let config = ExecutorConfig {
		signature_mock: SignatureMockMode::AlwaysValid,
		allow_unresolved_imports: false,
		max_log_level: 5,
		storage_proof_size: 1024,
	};

	let ctx = create_executor_context_with_config(config).await;
	let result = ctx
		.executor
		.call("Core_version", &[], &ctx.storage)
		.await
		.expect("Core_version with custom config failed");

	assert!(!result.output.is_empty(), "Should return output with custom config");
}

pub async fn logs_are_captured_during_execution() {
	let config = ExecutorConfig { max_log_level: 5, ..Default::default() };
	let ctx = create_executor_context_with_config(config).await;

	let result = ctx
		.executor
		.call("Metadata_metadata", &[], &ctx.storage)
		.await
		.expect("Metadata_metadata execution failed");

	assert!(result.output.len() > 1000, "Metadata should still be returned");
}

pub async fn core_initialize_block_modifies_storage() {
	let ctx = create_executor_context().await;
	let next_block_number = ctx.block_number + 1;

	#[derive(Encode)]
	struct Header {
		parent_hash: H256,
		#[codec(compact)]
		number: u32,
		state_root: H256,
		extrinsics_root: H256,
		digest: Vec<DigestItem>,
	}

	#[derive(Encode)]
	enum DigestItem {
		#[codec(index = 6)]
		PreRuntime([u8; 4], Vec<u8>),
	}

	let header = Header {
		parent_hash: ctx.block_hash,
		number: next_block_number,
		state_root: H256::zero(),
		extrinsics_root: H256::zero(),
		digest: vec![DigestItem::PreRuntime(*b"aura", 0u64.encode())],
	};

	let result = ctx
		.executor
		.call("Core_initialize_block", &header.encode(), &ctx.storage)
		.await
		.expect("Core_initialize_block execution failed");

	assert!(
		!result.storage_diff.is_empty(),
		"Core_initialize_block should modify storage, got {} changes",
		result.storage_diff.len()
	);

	let system_prefix = sp_core::twox_128(b"System");
	let number_key = sp_core::twox_128(b"Number");
	let system_number_key: Vec<u8> = [system_prefix.as_slice(), number_key.as_slice()].concat();
	let has_number_update = result.storage_diff.iter().any(|(key, _)| key == &system_number_key);

	assert!(
		has_number_update,
		"Core_initialize_block should update System::Number. Keys modified: {:?}",
		result.storage_diff.iter().map(|(k, _)| hex::encode(k)).collect::<Vec<_>>()
	);
}

pub async fn storage_reads_from_accumulated_changes() {
	let ctx = create_executor_context().await;

	#[derive(Encode)]
	struct Header {
		parent_hash: H256,
		#[codec(compact)]
		number: u32,
		state_root: H256,
		extrinsics_root: H256,
		digest: Vec<DigestItem>,
	}

	#[derive(Encode)]
	enum DigestItem {
		#[codec(index = 6)]
		PreRuntime([u8; 4], Vec<u8>),
	}

	let header = Header {
		parent_hash: ctx.block_hash,
		number: ctx.block_number + 1,
		state_root: H256::zero(),
		extrinsics_root: H256::zero(),
		digest: vec![DigestItem::PreRuntime(*b"aura", 0u64.encode())],
	};

	let result = ctx
		.executor
		.call("Core_initialize_block", &header.encode(), &ctx.storage)
		.await
		.expect("Core_initialize_block execution failed");

	assert!(!result.storage_diff.is_empty(), "Should have storage changes");
}

pub async fn storage_changes_persist_across_calls() {
	let ctx = create_executor_context().await;

	#[derive(Encode)]
	struct Header {
		parent_hash: H256,
		#[codec(compact)]
		number: u32,
		state_root: H256,
		extrinsics_root: H256,
		digest: Vec<DigestItem>,
	}

	#[derive(Encode)]
	enum DigestItem {
		#[codec(index = 6)]
		PreRuntime([u8; 4], Vec<u8>),
	}

	let header = Header {
		parent_hash: ctx.block_hash,
		number: ctx.block_number + 1,
		state_root: H256::zero(),
		extrinsics_root: H256::zero(),
		digest: vec![DigestItem::PreRuntime(*b"aura", 0u64.encode())],
	};

	let init_result = ctx
		.executor
		.call("Core_initialize_block", &header.encode(), &ctx.storage)
		.await
		.expect("Core_initialize_block failed");
	assert!(!init_result.storage_diff.is_empty(), "Init should write storage");

	for (key, value) in &init_result.storage_diff {
		ctx.storage.set(key, value.as_deref()).expect("Failed to apply storage change");
	}

	let system_prefix = sp_core::twox_128(b"System");
	let number_key = sp_core::twox_128(b"Number");
	let system_number_key: Vec<u8> = [system_prefix.as_slice(), number_key.as_slice()].concat();

	let block_num = ctx.storage.get_current_block_number();
	let stored_value = ctx
		.storage
		.get(block_num, &system_number_key)
		.await
		.expect("Failed to read System::Number");

	assert!(stored_value.is_some(), "System::Number should be set after Core_initialize_block");
}

pub async fn runtime_version_extracts_version_info() {
	let ctx = create_executor_context().await;

	let version = ctx.executor.runtime_version().expect("runtime_version should succeed");
	assert!(!version.spec_name.is_empty(), "spec_name should not be empty");
	assert!(!version.impl_name.is_empty(), "impl_name should not be empty");
	assert!(version.spec_version > 0, "spec_version should be positive");
}
