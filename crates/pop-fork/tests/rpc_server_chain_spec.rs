// SPDX-License-Identifier: GPL-3.0

//! Integration tests for rpc_server chain_spec methods.

#![cfg(feature = "integration-tests")]

use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
use pop_fork::testing::TestContext;

#[tokio::test(flavor = "multi_thread")]
async fn chain_spec_chain_name_returns_string() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let name: String = client
		.request("chainSpec_v1_chainName", rpc_params![])
		.await
		.expect("RPC call failed");

	// Chain name should not be empty
	assert!(!name.is_empty(), "Chain name should not be empty");

	// Should match blockchain's chain_name
	assert_eq!(name, "ink-node");
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_spec_genesis_hash_returns_valid_hex_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let hash: String = client
		.request("chainSpec_v1_genesisHash", rpc_params![])
		.await
		.expect("RPC call failed");

	// Hash should be properly formatted
	assert!(hash.starts_with("0x"), "Hash should start with 0x");
	assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_spec_genesis_hash_matches_archive() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Get genesis hash via chainSpec
	let chain_spec_hash: String = client
		.request("chainSpec_v1_genesisHash", rpc_params![])
		.await
		.expect("chainSpec RPC call failed");

	// Get genesis hash via archive
	let archive_hash: String = client
		.request("archive_v1_genesisHash", rpc_params![])
		.await
		.expect("archive RPC call failed");

	// Both should return the same value
	assert_eq!(chain_spec_hash, archive_hash, "chainSpec and archive genesis hashes should match");
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_spec_properties_returns_json_or_null() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let properties: Option<serde_json::Value> = client
		.request("chainSpec_v1_properties", rpc_params![])
		.await
		.expect("RPC call failed");

	// Properties can be Some or None, both are valid
	// If present, should be an object
	if let Some(props) = &properties {
		assert!(props.is_object(), "Properties should be a JSON object");
	}
}
