// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs, dead_code)]

//! Integration tests for rpc_server archive methods.

use crate::{
	rpc_server::types::{
		ArchiveCallResult, ArchiveStorageDiffResult, ArchiveStorageResult, StorageDiffType,
	},
	testing::TestContext,
};
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

const SYSTEM_PALLET: &[u8] = b"System";
const NUMBER_STORAGE: &[u8] = b"Number";

pub async fn archive_finalized_height_returns_correct_value() {
	let ctx = TestContext::for_rpc_server().await;
	let expected_block_height = ctx.blockchain().head_number().await;
	archive_finalized_height_returns_correct_value_at(&ctx.ws_url(), expected_block_height).await;

	// Create a new block
	ctx.blockchain().build_empty_block().await.unwrap();
	archive_finalized_height_returns_correct_value_at(&ctx.ws_url(), expected_block_height + 1)
		.await;
}

pub async fn archive_finalized_height_returns_correct_value_at(ws_url: &str, expected_height: u32) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let height: u32 = client
		.request("archive_v1_finalizedHeight", rpc_params![])
		.await
		.expect("RPC call failed");
	assert_eq!(height, expected_height);
}

pub async fn archive_genesis_hash_returns_valid_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let expected_hash = ctx
		.blockchain()
		.block_hash_at(0)
		.await
		.expect("Failed to get genesis hash")
		.expect("Genesis block should exist");
	let expected = format!("0x{}", hex::encode(expected_hash.as_bytes()));
	archive_genesis_hash_returns_valid_hash_at(&ctx.ws_url(), &expected).await;
}

pub async fn archive_genesis_hash_returns_valid_hash_at(ws_url: &str, expected_hash_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let hash: String = client
		.request("archive_v1_genesisHash", rpc_params![])
		.await
		.expect("RPC call failed");

	// Hash should be properly formatted
	assert!(hash.starts_with("0x"), "Hash should start with 0x");
	assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");

	assert_eq!(hash, expected_hash_hex);
}

pub async fn archive_hash_by_height_returns_hash_at_different_heights() {
	let ctx = TestContext::for_rpc_server().await;

	let block_1 = ctx.blockchain().build_empty_block().await.unwrap();
	let block_2 = ctx.blockchain().build_empty_block().await.unwrap();

	let fork_height = ctx.blockchain().fork_point_number();

	archive_hash_by_height_returns_hash_at_height_at(
		&ctx.ws_url(),
		fork_height,
		&format!("0x{}", hex::encode(ctx.blockchain().fork_point().as_bytes())),
	)
	.await;
	archive_hash_by_height_returns_hash_at_height_at(
		&ctx.ws_url(),
		block_1.number,
		&format!("0x{}", hex::encode(block_1.hash.as_bytes())),
	)
	.await;
	archive_hash_by_height_returns_hash_at_height_at(
		&ctx.ws_url(),
		block_2.number,
		&format!("0x{}", hex::encode(block_2.hash.as_bytes())),
	)
	.await;

	// Get historical hash (if fork_point isn't 0)
	if fork_height > 0 {
		let expected = ctx
			.blockchain()
			.block_hash_at(fork_height - 1)
			.await
			.expect("historical hash query should work")
			.expect("historical hash should exist");
		archive_hash_by_height_returns_hash_at_height_at(
			&ctx.ws_url(),
			fork_height - 1,
			&format!("0x{}", hex::encode(expected.as_bytes())),
		)
		.await;
	}
}

pub async fn archive_hash_by_height_returns_hash_at_height_at(
	ws_url: &str,
	height: u32,
	expected_hash_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Option<Vec<String>> = client
		.request("archive_v1_hashByHeight", rpc_params![height])
		.await
		.expect("RPC call failed");
	let result = result.expect("Expected hash result");
	assert_eq!(result.len(), 1, "Should return exactly one hash");
	assert!(result[0].starts_with("0x"), "Hash should start with 0x");
	assert_eq!(result[0], expected_hash_hex);
}

pub async fn archive_hash_by_height_returns_none_for_unknown_height() {
	let ctx = TestContext::for_rpc_server().await;
	archive_hash_by_height_returns_none_for_unknown_height_at(&ctx.ws_url(), 999_999_999u32).await;
}

pub async fn archive_hash_by_height_returns_none_for_unknown_height_at(ws_url: &str, height: u32) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Option<Vec<String>> = client
		.request("archive_v1_hashByHeight", rpc_params![height])
		.await
		.expect("RPC call failed");
	assert!(result.is_none(), "Should return none array for unknown height");
}

pub async fn archive_header_returns_header_for_head_hash() {
	let ctx = TestContext::for_rpc_server().await;

	// Build a block so we have a locally-built header
	ctx.blockchain().build_empty_block().await.unwrap();

	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	archive_header_returns_header_for_hash_at(&ctx.ws_url(), &head_hash).await;
}

pub async fn archive_header_returns_none_for_unknown_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
	archive_header_returns_none_for_unknown_hash_at(&ctx.ws_url(), unknown_hash).await;
}

pub async fn archive_header_returns_header_for_fork_point() {
	let ctx = TestContext::for_rpc_server().await;
	let fork_point_hash = format!("0x{}", hex::encode(ctx.blockchain().fork_point().0));
	archive_header_returns_header_for_hash_at(&ctx.ws_url(), &fork_point_hash).await;
}

pub async fn archive_header_returns_header_for_parent_block() {
	let ctx = TestContext::for_rpc_server().await;

	// Build two blocks
	let block1 = ctx.blockchain().build_empty_block().await.unwrap();
	let _block2 = ctx.blockchain().build_empty_block().await.unwrap();

	let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));
	archive_header_returns_header_for_hash_at(&ctx.ws_url(), &block1_hash).await;
}

pub async fn archive_header_returns_header_for_hash_at(ws_url: &str, hash_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let header: Option<String> = client
		.request("archive_v1_header", rpc_params![hash_hex])
		.await
		.expect("RPC call failed");
	assert!(header.is_some(), "Should return header for hash");
	let header_hex = header.expect("header should exist");
	assert!(header_hex.starts_with("0x"), "Header should be hex-encoded");
}

pub async fn archive_header_returns_none_for_unknown_hash_at(ws_url: &str, unknown_hash: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let header: Option<String> = client
		.request("archive_v1_header", rpc_params![unknown_hash])
		.await
		.expect("RPC call failed");
	assert!(header.is_none(), "Should return None for unknown hash");
}

pub async fn archive_header_is_idempotent_over_finalized_blocks() {
	let ctx = TestContext::for_rpc_server().await;

	// Build a few blocks
	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();

	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");
	let height: u32 = client
		.request("archive_v1_finalizedHeight", rpc_params![])
		.await
		.expect("RPC call failed");

	let hash: Option<Vec<String>> = client
		.request("archive_v1_hashByHeight", rpc_params![height])
		.await
		.expect("RPC call failed");

	let hash = hash.unwrap().pop();
	archive_header_is_idempotent_for_hash_at(
		&ctx.ws_url(),
		hash.as_deref().expect("hash should exist"),
	)
	.await;
}

pub async fn archive_header_is_idempotent_for_hash_at(ws_url: &str, hash_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let header_1: Option<String> = client
		.request("archive_v1_header", rpc_params![hash_hex])
		.await
		.expect("RPC call failed");
	let header_2: Option<String> = client
		.request("archive_v1_header", rpc_params![hash_hex])
		.await
		.expect("RPC call failed");
	assert_eq!(header_1, header_2, "Header should be idempotent");
}

pub async fn archive_body_returns_extrinsics_for_valid_hashes() {
	let ctx = TestContext::for_rpc_server().await;

	let fork_point_hash = format!("0x{}", hex::encode(ctx.blockchain().fork_point().0));
	let fork_point_body =
		archive_body_returns_extrinsics_for_hash_at(&ctx.ws_url(), &fork_point_hash).await;

	// Build a few blocks
	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();

	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	let body = archive_body_returns_extrinsics_for_hash_at(&ctx.ws_url(), &head_hash).await;

	// The latest body is just the mocked timestamp, so should be different from the fork point
	// body
	assert_ne!(fork_point_body, body);
}

pub async fn archive_body_is_idempotent_over_finalized_blocks() {
	let ctx = TestContext::for_rpc_server().await;

	// Build a few blocks
	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();

	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");
	let height: u32 = client
		.request("archive_v1_finalizedHeight", rpc_params![])
		.await
		.expect("RPC call failed");

	let hash: Option<Vec<String>> = client
		.request("archive_v1_hashByHeight", rpc_params![height])
		.await
		.expect("RPC call failed");

	let hash = hash.unwrap().pop();
	archive_body_is_idempotent_for_hash_at(
		&ctx.ws_url(),
		hash.as_deref().expect("hash should exist"),
	)
	.await;
}

pub async fn archive_body_returns_none_for_unknown_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
	archive_body_returns_none_for_unknown_hash_at(&ctx.ws_url(), unknown_hash).await;
}

pub async fn archive_body_returns_extrinsics_for_hash_at(
	ws_url: &str,
	hash_hex: &str,
) -> Vec<String> {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let body: Option<Vec<String>> = client
		.request("archive_v1_body", rpc_params![hash_hex])
		.await
		.expect("RPC call failed");
	body.expect("Body should exist for valid hash")
}

pub async fn archive_body_is_idempotent_for_hash_at(ws_url: &str, hash_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let body_1: Option<Vec<String>> = client
		.request("archive_v1_body", rpc_params![hash_hex])
		.await
		.expect("RPC call failed");
	let body_2: Option<Vec<String>> = client
		.request("archive_v1_body", rpc_params![hash_hex])
		.await
		.expect("RPC call failed");
	assert_eq!(body_1, body_2);
}

pub async fn archive_body_returns_none_for_unknown_hash_at(ws_url: &str, unknown_hash: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let body: Option<Vec<String>> = client
		.request("archive_v1_body", rpc_params![unknown_hash])
		.await
		.expect("RPC call failed");
	assert!(body.is_none(), "Should return None for unknown hash");
}

pub async fn archive_call_executes_runtime_api() {
	let ctx = TestContext::for_rpc_server().await;
	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	archive_call_executes_runtime_api_at(&ctx.ws_url(), &head_hash, "Core_version", "0x").await;
}

pub async fn archive_call_returns_error_for_invalid_function() {
	let ctx = TestContext::for_rpc_server().await;
	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	archive_call_returns_error_for_invalid_function_at(
		&ctx.ws_url(),
		&head_hash,
		"NonExistent_function",
		"0x",
	)
	.await;
}

pub async fn archive_call_returns_null_for_unknown_block() {
	let ctx = TestContext::for_rpc_server().await;
	let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
	archive_call_returns_null_for_unknown_block_at(
		&ctx.ws_url(),
		unknown_hash,
		"Core_version",
		"0x",
	)
	.await;
}

pub async fn archive_call_executes_at_specific_block() {
	let ctx = TestContext::for_rpc_server().await;
	// Get fork point hash
	let fork_hash = format!("0x{}", hex::encode(ctx.blockchain().fork_point().as_bytes()));

	// Build a new block so we have multiple blocks
	ctx.blockchain().build_empty_block().await.unwrap();

	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	archive_call_executes_runtime_api_at(&ctx.ws_url(), &fork_hash, "Core_version", "0x").await;
	archive_call_executes_runtime_api_at(&ctx.ws_url(), &head_hash, "Core_version", "0x").await;
}

pub async fn archive_call_rejects_invalid_hex_hash() {
	let ctx = TestContext::for_rpc_server().await;
	archive_call_rejects_invalid_hex_hash_at(&ctx.ws_url(), "not_valid_hex", "Core_version", "0x")
		.await;
}

pub async fn archive_call_executes_runtime_api_at(
	ws_url: &str,
	hash_hex: &str,
	function_name: &str,
	params_hex: &str,
) {
	let client = WsClientBuilder::default()
		.request_timeout(std::time::Duration::from_secs(120))
		.build(ws_url)
		.await
		.expect("Failed to connect");
	let result: Option<serde_json::Value> = client
		.request("archive_v1_call", rpc_params![hash_hex, function_name, params_hex])
		.await
		.expect("RPC call should succeed");
	let result = result.expect("Should return result for block");
	assert_eq!(result.get("success").and_then(|v| v.as_bool()), Some(true));
	let value = result.get("value").and_then(|v| v.as_str());
	assert!(value.is_some(), "Should have value field");
	assert!(value.expect("value should exist").starts_with("0x"), "Value should be hex-encoded");
}

pub async fn archive_call_returns_error_for_invalid_function_at(
	ws_url: &str,
	hash_hex: &str,
	function_name: &str,
	params_hex: &str,
) {
	let client = WsClientBuilder::default()
		.request_timeout(std::time::Duration::from_secs(120))
		.build(ws_url)
		.await
		.expect("Failed to connect");
	let result: Option<serde_json::Value> = client
		.request("archive_v1_call", rpc_params![hash_hex, function_name, params_hex])
		.await
		.expect("RPC call failed");
	let result = result.expect("Should return result for valid block hash");
	assert_eq!(result.get("success").and_then(|v| v.as_bool()), Some(false));
	assert!(result.get("error").is_some(), "Should have error field");
}

pub async fn archive_call_returns_null_for_unknown_block_at(
	ws_url: &str,
	hash_hex: &str,
	function_name: &str,
	params_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Option<serde_json::Value> = client
		.request("archive_v1_call", rpc_params![hash_hex, function_name, params_hex])
		.await
		.expect("RPC call failed");
	assert!(result.is_none(), "Should return null for unknown block hash");
}

pub async fn archive_call_rejects_invalid_hex_hash_at(
	ws_url: &str,
	hash_hex: &str,
	function_name: &str,
	params_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Result<Option<serde_json::Value>, _> = client
		.request("archive_v1_call", rpc_params![hash_hex, function_name, params_hex])
		.await;
	assert!(result.is_err(), "Should reject invalid hex hash");
}

pub async fn archive_storage_returns_value_for_existing_key() {
	let ctx = TestContext::for_rpc_server().await;
	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	let mut key = Vec::new();
	key.extend(sp_core::twox_128(SYSTEM_PALLET));
	key.extend(sp_core::twox_128(NUMBER_STORAGE));
	let key_hex = format!("0x{}", hex::encode(&key));
	archive_storage_returns_value_for_existing_key_at(&ctx.ws_url(), &head_hash, &key_hex).await;
}

pub async fn archive_storage_returns_value_for_existing_key_at(
	ws_url: &str,
	block_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"type": "value"
	})];

	let result: ArchiveStorageResult = client
		.request("archive_v1_storage", rpc_params![block_hash_hex, items, Option::<String>::None])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageResult::Ok { items } => {
			assert_eq!(items.len(), 1, "Should return one item");
			assert!(items[0].value.is_some(), "Value should be present");
		},
		_ => panic!("Expected Ok result"),
	}
}

pub async fn archive_storage_returns_none_for_nonexistent_key() {
	let ctx = TestContext::for_rpc_server().await;
	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	let key_hex = format!("0x{}", hex::encode(b"nonexistent_key_12345"));
	archive_storage_returns_none_for_nonexistent_key_at(&ctx.ws_url(), &head_hash, &key_hex).await;
}

pub async fn archive_storage_returns_none_for_nonexistent_key_at(
	ws_url: &str,
	block_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"type": "value"
	})];

	let result: ArchiveStorageResult = client
		.request("archive_v1_storage", rpc_params![block_hash_hex, items, Option::<String>::None])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageResult::Ok { items } => {
			assert_eq!(items.len(), 1, "Should return one item");
			assert!(items[0].value.is_none(), "Value should be None for non-existent key");
		},
		_ => panic!("Expected Ok result"),
	}
}

pub async fn archive_header_rejects_invalid_hex() {
	let ctx = TestContext::for_rpc_server().await;
	archive_header_rejects_invalid_hex_at(&ctx.ws_url(), "not_valid_hex").await;
}

pub async fn archive_header_rejects_invalid_hex_at(ws_url: &str, invalid_hash: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Result<Option<String>, _> =
		client.request("archive_v1_header", rpc_params![invalid_hash]).await;
	assert!(result.is_err(), "Should reject invalid hex");
}

pub async fn archive_call_rejects_invalid_hex_parameters() {
	let ctx = TestContext::for_rpc_server().await;
	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	archive_call_rejects_invalid_hex_parameters_at(&ctx.ws_url(), &head_hash, "Core_version").await;
}

pub async fn archive_call_rejects_invalid_hex_parameters_at(
	ws_url: &str,
	hash_hex: &str,
	function_name: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Result<Option<serde_json::Value>, _> = client
		.request("archive_v1_call", rpc_params![hash_hex, function_name, "not_hex"])
		.await;
	assert!(result.is_err(), "Should reject invalid hex parameters");
}

/// Verifies that calling `Core_initialize_block` via `archive_v1_call` RPC does NOT
/// persist storage changes.
///
/// `Core_initialize_block` writes to `System::Number` and other storage keys during
/// block initialization. This test verifies those changes are discarded after the call.
pub async fn archive_call_does_not_persist_storage_changes() {
	use crate::{DigestItem, consensus_engine, create_next_header};

	let ctx = TestContext::for_rpc_server().await;
	let head = ctx.blockchain().head().await;
	let head_hash = format!("0x{}", hex::encode(head.hash.as_bytes()));
	let system_number_key: Vec<u8> =
		[sp_core::twox_128(SYSTEM_PALLET).as_slice(), sp_core::twox_128(NUMBER_STORAGE).as_slice()]
			.concat();
	let header = create_next_header(
		&head,
		vec![DigestItem::PreRuntime(consensus_engine::AURA, 0u64.to_le_bytes().to_vec())],
	);
	let header_hex = format!("0x{}", hex::encode(&header));
	let system_number_key_hex = format!("0x{}", hex::encode(system_number_key));
	archive_call_does_not_persist_storage_changes_at(
		&ctx.ws_url(),
		&head_hash,
		&header_hex,
		&system_number_key_hex,
	)
	.await;
}

pub async fn archive_call_does_not_persist_storage_changes_at(
	ws_url: &str,
	head_hash_hex: &str,
	header_hex: &str,
	system_number_key_hex: &str,
) {
	let client = WsClientBuilder::default()
		.request_timeout(std::time::Duration::from_secs(120))
		.build(ws_url)
		.await
		.expect("Failed to connect");
	let query_items = vec![serde_json::json!({
		"key": system_number_key_hex,
		"type": "value"
	})];
	let before: ArchiveStorageResult = client
		.request(
			"archive_v1_storage",
			rpc_params![head_hash_hex, query_items.clone(), Option::<String>::None],
		)
		.await
		.expect("archive_v1_storage before call should succeed");
	let init_result: Option<ArchiveCallResult> = client
		.request("archive_v1_call", rpc_params![head_hash_hex, "Core_initialize_block", header_hex])
		.await
		.expect("Core_initialize_block RPC call failed");
	let init_result = init_result.expect("Block should exist");
	assert!(init_result.success, "Core_initialize_block should succeed: {:?}", init_result.error);
	let after: ArchiveStorageResult = client
		.request(
			"archive_v1_storage",
			rpc_params![head_hash_hex, query_items, Option::<String>::None],
		)
		.await
		.expect("archive_v1_storage after call should succeed");
	let before_items = match before {
		ArchiveStorageResult::Ok { items } => items,
		ArchiveStorageResult::Err { error } => panic!("Unexpected pre-call storage error: {error}"),
	};
	let after_items = match after {
		ArchiveStorageResult::Ok { items } => items,
		ArchiveStorageResult::Err { error } =>
			panic!("Unexpected post-call storage error: {error}"),
	};
	assert_eq!(before_items.len(), 1, "Expected one System::Number item before call");
	assert_eq!(after_items.len(), 1, "Expected one System::Number item after call");
	assert_eq!(
		before_items[0].value, after_items[0].value,
		"Storage should not persist changes from archive_v1_call",
	);
}

pub async fn archive_storage_returns_hash_when_requested() {
	let ctx = TestContext::for_rpc_server().await;
	let head_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	let mut key = Vec::new();
	key.extend(sp_core::twox_128(SYSTEM_PALLET));
	key.extend(sp_core::twox_128(NUMBER_STORAGE));
	let key_hex = format!("0x{}", hex::encode(&key));
	archive_storage_returns_hash_when_requested_at(&ctx.ws_url(), &head_hash, &key_hex).await;
}

pub async fn archive_storage_returns_hash_when_requested_at(
	ws_url: &str,
	block_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"type": "hash"
	})];

	let result: ArchiveStorageResult = client
		.request("archive_v1_storage", rpc_params![block_hash_hex, items, Option::<String>::None])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageResult::Ok { items } => {
			assert_eq!(items.len(), 1, "Should return one item");
			assert!(items[0].hash.is_some(), "Hash should be present");
			assert!(items[0].value.is_none(), "Value should not be present");
			let hash = items[0].hash.as_ref().unwrap();
			assert!(hash.starts_with("0x"), "Hash should be hex-encoded");
			assert_eq!(hash.len(), 66, "Hash should be 32 bytes (0x + 64 hex chars)");
		},
		_ => panic!("Expected Ok result"),
	}
}

pub async fn archive_storage_queries_at_specific_block() {
	let ctx = TestContext::for_rpc_server().await;
	// Build a block to change state
	ctx.blockchain().build_empty_block().await.unwrap();
	let block1_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));

	// Build another block
	ctx.blockchain().build_empty_block().await.unwrap();
	let block2_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));

	let mut key = Vec::new();
	key.extend(sp_core::twox_128(SYSTEM_PALLET));
	key.extend(sp_core::twox_128(NUMBER_STORAGE));
	let key_hex = format!("0x{}", hex::encode(&key));
	archive_storage_queries_at_specific_block_at(
		&ctx.ws_url(),
		&block1_hash,
		&block2_hash,
		&key_hex,
	)
	.await;
}

pub async fn archive_storage_queries_at_specific_block_at(
	ws_url: &str,
	first_block_hash_hex: &str,
	second_block_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({ "key": key_hex, "type": "value" })];

	let result1: ArchiveStorageResult = client
		.request(
			"archive_v1_storage",
			rpc_params![first_block_hash_hex, items.clone(), Option::<String>::None],
		)
		.await
		.expect("RPC call failed");

	let result2: ArchiveStorageResult = client
		.request(
			"archive_v1_storage",
			rpc_params![second_block_hash_hex, items, Option::<String>::None],
		)
		.await
		.expect("RPC call failed");

	// The block numbers should be different
	match (result1, result2) {
		(
			ArchiveStorageResult::Ok { items: items1 },
			ArchiveStorageResult::Ok { items: items2 },
		) => {
			assert_ne!(items1[0].value, items2[0].value, "Block numbers should differ");
		},
		_ => panic!("Expected Ok results"),
	}
}

pub async fn archive_storage_returns_error_for_unknown_block() {
	let ctx = TestContext::for_rpc_server().await;
	let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
	archive_storage_returns_error_for_unknown_block_at(&ctx.ws_url(), unknown_hash, "0x1234").await;
}

pub async fn archive_storage_returns_error_for_unknown_block_at(
	ws_url: &str,
	unknown_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({ "key": key_hex, "type": "value" })];
	let result: ArchiveStorageResult = client
		.request("archive_v1_storage", rpc_params![unknown_hash_hex, items, Option::<String>::None])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageResult::Err { error } => {
			assert!(
				error.contains("not found") || error.contains("Block"),
				"Should indicate block not found"
			);
		},
		_ => panic!("Expected Err result for unknown block"),
	}
}

pub async fn archive_storage_diff_detects_modified_value() {
	let ctx = TestContext::for_rpc_server().await;
	let test_key = b"test_storage_diff_key";
	let test_key_hex = format!("0x{}", hex::encode(test_key));

	// Set initial value and build first block
	ctx.blockchain().set_storage_for_testing(test_key, Some(b"value1")).await;
	let block1 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

	// Set modified value and build second block
	ctx.blockchain().set_storage_for_testing(test_key, Some(b"value2")).await;
	let block2 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));
	archive_storage_diff_detects_modified_value_at(
		&ctx.ws_url(),
		&block2_hash,
		&block1_hash,
		&test_key_hex,
		&format!("0x{}", hex::encode(b"value2")),
	)
	.await;
}

pub async fn archive_storage_diff_detects_modified_value_at(
	ws_url: &str,
	current_hash_hex: &str,
	previous_hash_hex: &str,
	key_hex: &str,
	expected_value_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"returnType": "value"
	})];

	let result: ArchiveStorageDiffResult = client
		.request("archive_v1_storageDiff", rpc_params![current_hash_hex, items, previous_hash_hex])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Ok { items } => {
			assert_eq!(items.len(), 1, "Should return one modified item");
			assert_eq!(items[0].key, key_hex);
			assert_eq!(items[0].diff_type, StorageDiffType::Modified);
			assert!(items[0].value.is_some(), "Value should be present");
			assert_eq!(items[0].value.as_ref().unwrap(), expected_value_hex);
		},
		ArchiveStorageDiffResult::Err { error } => panic!("Expected Ok result, got error: {error}"),
	}
}

pub async fn archive_storage_diff_returns_empty_for_unchanged_keys() {
	let ctx = TestContext::for_rpc_server().await;
	let test_key = b"test_unchanged_key";
	let test_key_hex = format!("0x{}", hex::encode(test_key));

	// Set value and build first block
	ctx.blockchain()
		.set_storage_for_testing(test_key, Some(b"constant_value"))
		.await;
	let block1 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

	// Build second block without changing the value
	let block2 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));
	archive_storage_diff_returns_empty_for_unchanged_keys_at(
		&ctx.ws_url(),
		&block2_hash,
		&block1_hash,
		&test_key_hex,
	)
	.await;
}

pub async fn archive_storage_diff_returns_empty_for_unchanged_keys_at(
	ws_url: &str,
	current_hash_hex: &str,
	previous_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"returnType": "value"
	})];

	let result: ArchiveStorageDiffResult = client
		.request("archive_v1_storageDiff", rpc_params![current_hash_hex, items, previous_hash_hex])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Ok { items } => {
			assert!(items.is_empty(), "Should return empty for unchanged keys");
		},
		ArchiveStorageDiffResult::Err { error } => panic!("Expected Ok result, got error: {error}"),
	}
}

pub async fn archive_storage_diff_returns_added_for_new_key() {
	let ctx = TestContext::for_rpc_server().await;
	// Build first block without the key
	let block1 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

	// Add a new key and build second block
	let test_key = b"test_added_key";
	let test_key_hex = format!("0x{}", hex::encode(test_key));
	ctx.blockchain().set_storage_for_testing(test_key, Some(b"new_value")).await;
	let block2 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));
	archive_storage_diff_returns_added_for_new_key_at(
		&ctx.ws_url(),
		&block2_hash,
		&block1_hash,
		&test_key_hex,
		&format!("0x{}", hex::encode(b"new_value")),
	)
	.await;
}

pub async fn archive_storage_diff_returns_added_for_new_key_at(
	ws_url: &str,
	current_hash_hex: &str,
	previous_hash_hex: &str,
	key_hex: &str,
	expected_value_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"returnType": "value"
	})];

	let result: ArchiveStorageDiffResult = client
		.request("archive_v1_storageDiff", rpc_params![current_hash_hex, items, previous_hash_hex])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Ok { items } => {
			assert_eq!(items.len(), 1, "Should return one added item");
			assert_eq!(items[0].key, key_hex);
			assert_eq!(items[0].diff_type, StorageDiffType::Added);
			assert!(items[0].value.is_some(), "Value should be present");
			assert_eq!(items[0].value.as_ref().unwrap(), expected_value_hex);
		},
		ArchiveStorageDiffResult::Err { error } => panic!("Expected Ok result, got error: {error}"),
	}
}

pub async fn archive_storage_diff_returns_deleted_for_removed_key() {
	let ctx = TestContext::for_rpc_server().await;
	// Add a key and build first block
	let test_key = b"test_deleted_key";
	let test_key_hex = format!("0x{}", hex::encode(test_key));
	ctx.blockchain()
		.set_storage_for_testing(test_key, Some(b"will_be_deleted"))
		.await;
	let block1 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

	// Delete the key and build second block
	ctx.blockchain().set_storage_for_testing(test_key, None).await;
	let block2 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));
	archive_storage_diff_returns_deleted_for_removed_key_at(
		&ctx.ws_url(),
		&block2_hash,
		&block1_hash,
		&test_key_hex,
	)
	.await;
}

pub async fn archive_storage_diff_returns_deleted_for_removed_key_at(
	ws_url: &str,
	current_hash_hex: &str,
	previous_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"returnType": "value"
	})];

	let result: ArchiveStorageDiffResult = client
		.request("archive_v1_storageDiff", rpc_params![current_hash_hex, items, previous_hash_hex])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Ok { items } => {
			assert_eq!(items.len(), 1, "Should return one deleted item");
			assert_eq!(items[0].key, key_hex);
			assert_eq!(items[0].diff_type, StorageDiffType::Deleted);
			assert!(items[0].value.is_none(), "Value should be None for deleted key");
			assert!(items[0].hash.is_none(), "Hash should be None for deleted key");
		},
		ArchiveStorageDiffResult::Err { error } => panic!("Expected Ok result, got error: {error}"),
	}
}

pub async fn archive_storage_diff_returns_hash_when_requested() {
	let ctx = TestContext::for_rpc_server().await;
	let test_key = b"test_hash_key";
	let test_key_hex = format!("0x{}", hex::encode(test_key));

	// Set initial value and build first block
	ctx.blockchain().set_storage_for_testing(test_key, Some(b"value1")).await;
	let block1 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

	// Set modified value and build second block
	let new_value = b"value2";
	ctx.blockchain().set_storage_for_testing(test_key, Some(new_value)).await;
	let block2 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));
	archive_storage_diff_returns_hash_when_requested_at(
		&ctx.ws_url(),
		&block2_hash,
		&block1_hash,
		&test_key_hex,
		&format!("0x{}", hex::encode(sp_core::blake2_256(new_value))),
	)
	.await;
}

pub async fn archive_storage_diff_returns_hash_when_requested_at(
	ws_url: &str,
	current_hash_hex: &str,
	previous_hash_hex: &str,
	key_hex: &str,
	expected_hash_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"returnType": "hash"
	})];

	let result: ArchiveStorageDiffResult = client
		.request("archive_v1_storageDiff", rpc_params![current_hash_hex, items, previous_hash_hex])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Ok { items } => {
			assert_eq!(items.len(), 1, "Should return one modified item");
			assert_eq!(items[0].diff_type, StorageDiffType::Modified);
			assert!(items[0].value.is_none(), "Value should not be present");
			assert!(items[0].hash.is_some(), "Hash should be present");
			assert_eq!(items[0].hash.as_ref().unwrap(), expected_hash_hex);
		},
		ArchiveStorageDiffResult::Err { error } => panic!("Expected Ok result, got error: {error}"),
	}
}

pub async fn archive_storage_diff_returns_error_for_unknown_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
	let valid_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	archive_storage_diff_returns_error_for_unknown_hash_at(
		&ctx.ws_url(),
		unknown_hash,
		&valid_hash,
		"0x1234",
	)
	.await;
}

pub async fn archive_storage_diff_returns_error_for_unknown_hash_at(
	ws_url: &str,
	current_hash_hex: &str,
	previous_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({ "key": key_hex, "returnType": "value" })];
	let result: ArchiveStorageDiffResult = client
		.request("archive_v1_storageDiff", rpc_params![current_hash_hex, items, previous_hash_hex])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Err { error } => {
			assert!(
				error.contains("not found") || error.contains("Block"),
				"Should indicate block not found"
			);
		},
		_ => panic!("Expected Err result for unknown block"),
	}
}

pub async fn archive_storage_diff_returns_error_for_unknown_previous_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let valid_hash = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	let unknown_hash = "0x0000000000000000000000000000000000000000000000000000000000000001";
	archive_storage_diff_returns_error_for_unknown_previous_hash_at(
		&ctx.ws_url(),
		&valid_hash,
		unknown_hash,
		"0x1234",
	)
	.await;
}

pub async fn archive_storage_diff_returns_error_for_unknown_previous_hash_at(
	ws_url: &str,
	current_hash_hex: &str,
	previous_hash_hex: &str,
	key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({ "key": key_hex, "returnType": "value" })];
	let result: ArchiveStorageDiffResult = client
		.request("archive_v1_storageDiff", rpc_params![current_hash_hex, items, previous_hash_hex])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Err { error } => {
			assert!(
				error.contains("not found") || error.contains("Previous block"),
				"Should indicate previous block not found"
			);
		},
		_ => panic!("Expected Err result for unknown previous block"),
	}
}

pub async fn archive_storage_diff_uses_parent_when_previous_hash_omitted() {
	let ctx = TestContext::for_rpc_server().await;
	let test_key = b"test_parent_key";
	let test_key_hex = format!("0x{}", hex::encode(test_key));

	// Set initial value and build first block (parent)
	ctx.blockchain().set_storage_for_testing(test_key, Some(b"parent_value")).await;
	ctx.blockchain().build_empty_block().await.expect("Failed to build block");

	// Set modified value and build second block (child)
	ctx.blockchain().set_storage_for_testing(test_key, Some(b"child_value")).await;

	let child_block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let child_hash = format!("0x{}", hex::encode(child_block.hash.as_bytes()));
	archive_storage_diff_uses_parent_when_previous_hash_omitted_at(
		&ctx.ws_url(),
		&child_hash,
		&test_key_hex,
		&format!("0x{}", hex::encode(b"child_value")),
	)
	.await;
}

pub async fn archive_storage_diff_uses_parent_when_previous_hash_omitted_at(
	ws_url: &str,
	current_hash_hex: &str,
	key_hex: &str,
	expected_value_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![serde_json::json!({
		"key": key_hex,
		"returnType": "value"
	})];

	let result: ArchiveStorageDiffResult = client
		.request(
			"archive_v1_storageDiff",
			rpc_params![current_hash_hex, items, Option::<String>::None],
		)
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Ok { items } => {
			assert_eq!(items.len(), 1, "Should return one modified item");
			assert_eq!(items[0].diff_type, StorageDiffType::Modified);
			assert_eq!(items[0].value.as_ref().unwrap(), expected_value_hex);
		},
		ArchiveStorageDiffResult::Err { error } => panic!("Expected Ok result, got error: {error}"),
	}
}

pub async fn archive_storage_diff_handles_multiple_items() {
	let ctx = TestContext::for_rpc_server().await;
	// Create test keys
	let added_key = b"test_multi_added";
	let modified_key = b"test_multi_modified";
	let deleted_key = b"test_multi_deleted";
	let unchanged_key = b"test_multi_unchanged";

	// Set up initial state for block 1
	ctx.blockchain().set_storage_for_testing(modified_key, Some(b"old_value")).await;
	ctx.blockchain().set_storage_for_testing(deleted_key, Some(b"to_delete")).await;
	ctx.blockchain().set_storage_for_testing(unchanged_key, Some(b"constant")).await;
	let block1 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block1_hash = format!("0x{}", hex::encode(block1.hash.as_bytes()));

	// Modify state for block 2
	ctx.blockchain().set_storage_for_testing(added_key, Some(b"new_key")).await;
	ctx.blockchain().set_storage_for_testing(modified_key, Some(b"new_value")).await;
	ctx.blockchain().set_storage_for_testing(deleted_key, None).await;
	// unchanged_key stays the same
	let block2 = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block2_hash = format!("0x{}", hex::encode(block2.hash.as_bytes()));
	archive_storage_diff_handles_multiple_items_at(
		&ctx.ws_url(),
		&block2_hash,
		&block1_hash,
		&format!("0x{}", hex::encode(added_key)),
		&format!("0x{}", hex::encode(modified_key)),
		&format!("0x{}", hex::encode(deleted_key)),
		&format!("0x{}", hex::encode(unchanged_key)),
	)
	.await;
}

pub async fn archive_storage_diff_handles_multiple_items_at(
	ws_url: &str,
	current_hash_hex: &str,
	previous_hash_hex: &str,
	added_key_hex: &str,
	modified_key_hex: &str,
	deleted_key_hex: &str,
	unchanged_key_hex: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let items = vec![
		serde_json::json!({ "key": added_key_hex, "returnType": "value" }),
		serde_json::json!({ "key": modified_key_hex, "returnType": "value" }),
		serde_json::json!({ "key": deleted_key_hex, "returnType": "value" }),
		serde_json::json!({ "key": unchanged_key_hex, "returnType": "value" }),
	];

	let result: ArchiveStorageDiffResult = client
		.request("archive_v1_storageDiff", rpc_params![current_hash_hex, items, previous_hash_hex])
		.await
		.expect("RPC call failed");

	match result {
		ArchiveStorageDiffResult::Ok { items } => {
			// Should have 3 items (added, modified, deleted) but NOT unchanged
			assert_eq!(items.len(), 3, "Should return 3 changed items (not unchanged)");

			// Find each item by key
			let added = items.iter().find(|i| i.key == added_key_hex);
			let modified = items.iter().find(|i| i.key == modified_key_hex);
			let deleted = items.iter().find(|i| i.key == deleted_key_hex);
			let unchanged = items.iter().find(|i| i.key == unchanged_key_hex);

			assert!(added.is_some(), "Added key should be in results");
			assert_eq!(added.unwrap().diff_type, StorageDiffType::Added);

			assert!(modified.is_some(), "Modified key should be in results");
			assert_eq!(modified.unwrap().diff_type, StorageDiffType::Modified);

			assert!(deleted.is_some(), "Deleted key should be in results");
			assert_eq!(deleted.unwrap().diff_type, StorageDiffType::Deleted);

			assert!(unchanged.is_none(), "Unchanged key should NOT be in results");
		},
		ArchiveStorageDiffResult::Err { error } => panic!("Expected Ok result, got error: {error}"),
	}
}
