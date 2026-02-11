// SPDX-License-Identifier: GPL-3.0

//! Integration tests for rpc_server state methods.

#![cfg(feature = "integration-tests")]

use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
use pop_fork::{
	rpc_server::types::RuntimeVersion,
	testing::{TestContext, accounts::ALICE, helpers::account_storage_key},
};

#[tokio::test(flavor = "multi_thread")]
async fn state_get_storage_returns_value() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Query Alice's account storage (should exist on dev chain)
	let key = account_storage_key(&ALICE);
	let key_hex = format!("0x{}", hex::encode(&key));

	let result: Option<String> = client
		.request("state_getStorage", rpc_params![key_hex])
		.await
		.expect("RPC call failed");

	assert!(result.is_some(), "Alice's account should exist");
	let value = result.unwrap();
	assert!(value.starts_with("0x"), "Value should be hex encoded");
	assert!(value.len() > 2, "Value should not be empty");
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_storage_at_block_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let block = ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();

	// Get current head hash
	let block_hash_hex = format!("0x{}", hex::encode(block.hash.as_bytes()));

	// Query Alice's account storage at specific block
	let key = account_storage_key(&ALICE);
	let key_hex = format!("0x{}", hex::encode(&key));

	let result: Option<String> = client
		.request("state_getStorage", rpc_params![key_hex, block_hash_hex])
		.await
		.expect("RPC call failed");

	assert!(result.is_some(), "Alice's account should exist at block");
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_storage_returns_none_for_nonexistent_key() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Query a nonexistent storage key
	let fake_key = "0x0000000000000000000000000000000000000000000000000000000000000000";

	let result: Option<String> = client
		.request("state_getStorage", rpc_params![fake_key])
		.await
		.expect("RPC call failed");

	assert!(result.is_none(), "Nonexistent key should return None");
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_metadata_returns_metadata() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.request_timeout(std::time::Duration::from_secs(120))
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let result: String = client
		.request("state_getMetadata", rpc_params![])
		.await
		.expect("RPC call failed");

	assert!(result.starts_with("0x"), "Metadata should be hex encoded");
	// Metadata is large, just check it's substantial
	assert!(result.len() > 1000, "Metadata should be substantial in size");
	// Verify metadata magic number "meta" (0x6d657461) is at the start
	assert!(
		result.starts_with("0x6d657461"),
		"Metadata should start with magic number 'meta' (0x6d657461), got: {}",
		&result[..20.min(result.len())]
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_metadata_at_block_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.request_timeout(std::time::Duration::from_secs(120))
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Get current head hash
	let head_hash = ctx.blockchain().head_hash().await;
	let head_hash_hex = format!("0x{}", hex::encode(head_hash.as_bytes()));

	let result: String = client
		.request("state_getMetadata", rpc_params![head_hash_hex])
		.await
		.expect("RPC call failed");

	assert!(result.starts_with("0x"), "Metadata should be hex encoded");
	assert!(result.len() > 1000, "Metadata should be substantial in size");
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_runtime_version_returns_version() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let result: RuntimeVersion = client
		.request("state_getRuntimeVersion", rpc_params![])
		.await
		.expect("RPC call failed");

	// Verify we got a valid runtime version
	assert!(!result.spec_name.is_empty(), "Spec name should not be empty");
	assert!(!result.impl_name.is_empty(), "Impl name should not be empty");
	assert!(result.spec_version > 0, "Spec version should be positive");
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_runtime_version_at_block_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Get current head hash
	let head_hash = ctx.blockchain().head_hash().await;
	let head_hash_hex = format!("0x{}", hex::encode(head_hash.as_bytes()));

	let result: RuntimeVersion = client
		.request("state_getRuntimeVersion", rpc_params![head_hash_hex])
		.await
		.expect("RPC call failed");

	assert!(!result.spec_name.is_empty(), "Spec name should not be empty");
	assert!(result.spec_version > 0, "Spec version should be positive");
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_storage_invalid_hex_returns_error() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let result: Result<Option<String>, _> =
		client.request("state_getStorage", rpc_params!["not_valid_hex"]).await;

	assert!(result.is_err(), "Should fail with invalid hex key");
}

#[tokio::test(flavor = "multi_thread")]
async fn state_get_storage_invalid_block_hash_returns_error() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let key = account_storage_key(&ALICE);
	let key_hex = format!("0x{}", hex::encode(&key));
	let invalid_hash = "0x0000000000000000000000000000000000000000000000000000000000000000";

	let result: Result<Option<String>, _> =
		client.request("state_getStorage", rpc_params![key_hex, invalid_hash]).await;

	assert!(result.is_err(), "Should fail with invalid block hash");
}
