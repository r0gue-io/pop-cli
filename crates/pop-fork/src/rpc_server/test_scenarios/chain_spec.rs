// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

pub async fn chain_spec_chain_name_returns_string(ws_url: &str, expected: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let name: String = client
		.request("chainSpec_v1_chainName", rpc_params![])
		.await
		.expect("RPC call failed");

	assert!(!name.is_empty(), "Chain name should not be empty");
	assert_eq!(name, expected);
}

pub async fn chain_spec_genesis_hash_returns_valid_hex_hash(
	ws_url: &str,
	expected: Option<&str>,
) -> String {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let hash: String = client
		.request("chainSpec_v1_genesisHash", rpc_params![])
		.await
		.expect("RPC call failed");

	assert!(hash.starts_with("0x"), "Hash should start with 0x");
	assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");
	if let Some(expected_hash) = expected {
		assert_eq!(hash, expected_hash);
	}
	hash
}

pub async fn chain_spec_genesis_hash_matches_archive(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let chain_spec_hash: String = client
		.request("chainSpec_v1_genesisHash", rpc_params![])
		.await
		.expect("chainSpec RPC call failed");

	let archive_hash: String = client
		.request("archive_v1_genesisHash", rpc_params![])
		.await
		.expect("archive RPC call failed");

	assert_eq!(chain_spec_hash, archive_hash, "chainSpec and archive genesis hashes should match");
}

pub async fn chain_spec_properties_returns_json_or_null(
	ws_url: &str,
	expected: Option<serde_json::Value>,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let properties: Option<serde_json::Value> = client
		.request("chainSpec_v1_properties", rpc_params![])
		.await
		.expect("RPC call failed");

	if let Some(props) = &properties {
		assert!(props.is_object(), "Properties should be a JSON object");
	}
	if expected.is_some() {
		assert_eq!(properties, expected);
	}
}
