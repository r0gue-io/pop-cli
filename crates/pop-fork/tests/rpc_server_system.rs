// SPDX-License-Identifier: GPL-3.0

//! Integration tests for rpc_server system methods.

#![cfg(feature = "integration-tests")]

use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
use pop_fork::{rpc_server::types::SystemHealth, testing::TestContext};

#[tokio::test(flavor = "multi_thread")]
async fn chain_works() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let name: String =
		client.request("system_chain", rpc_params![]).await.expect("RPC call failed");

	// Chain name should not be empty
	assert!(!name.is_empty(), "Chain name should not be empty");

	// Should match blockchain's chain_name
	assert_eq!(name, "ink-node");
}

#[tokio::test(flavor = "multi_thread")]
async fn name_works() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let name: String = client.request("system_name", rpc_params![]).await.expect("RPC call failed");

	// Chain name should not be empty
	assert!(!name.is_empty(), "Chain name should not be empty");

	assert_eq!(name, "pop-fork");
}

#[tokio::test(flavor = "multi_thread")]
async fn version_works() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let version: String =
		client.request("system_version", rpc_params![]).await.expect("RPC call failed");

	assert_eq!(version, "1.0.0");
}

#[tokio::test(flavor = "multi_thread")]
async fn health_works() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let health: SystemHealth =
		client.request("system_health", rpc_params![]).await.expect("RPC call failed");

	// Should match blockchain's chain_name
	assert_eq!(health, SystemHealth::default());
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_spec_chain_name() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let name: String =
		client.request("system_chain", rpc_params![]).await.expect("RPC call failed");

	// Chain name should not be empty
	assert!(!name.is_empty(), "Chain name should not be empty");

	// Should match blockchain's chain_name
	assert_eq!(name, "ink-node");
}

#[tokio::test(flavor = "multi_thread")]
async fn properties_returns_json_or_null() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let properties: Option<serde_json::Value> = client
		.request("system_properties", rpc_params![])
		.await
		.expect("RPC call failed");

	// Properties can be Some or None, both are valid
	// If present, should be an object
	if let Some(props) = &properties {
		assert!(props.is_object(), "Properties should be a JSON object");
	}
}

/// Well-known dev account: Alice
const ALICE_SS58: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

#[tokio::test(flavor = "multi_thread")]
async fn account_next_index_returns_nonce() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Query Alice's nonce (should be 0 or some positive value on dev chain)
	let nonce: u32 = client
		.request("system_accountNextIndex", rpc_params![ALICE_SS58])
		.await
		.expect("RPC call failed");

	// Nonce should be a valid u32 (including 0)
	assert!(nonce < u32::MAX, "Nonce should be a valid value");
}

#[tokio::test(flavor = "multi_thread")]
async fn account_next_index_returns_zero_for_nonexistent() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Query a nonexistent account (random valid SS58 address)
	// This is a valid SS58 address but unlikely to have any balance
	let nonexistent_account = "5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuyUpnhM";

	let nonce: u32 = client
		.request("system_accountNextIndex", rpc_params![nonexistent_account])
		.await
		.expect("RPC call failed");

	// Nonexistent account should have nonce 0
	assert_eq!(nonce, 0, "Nonexistent account should have nonce 0");
}

#[tokio::test(flavor = "multi_thread")]
async fn account_next_index_invalid_address_returns_error() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Try with an invalid SS58 address
	let result: Result<u32, _> = client
		.request("system_accountNextIndex", rpc_params!["not_a_valid_address"])
		.await;

	assert!(result.is_err(), "Invalid SS58 address should return an error");
}
