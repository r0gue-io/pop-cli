// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

use crate::rpc_server::types::SystemHealth;
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

const ALICE_SS58: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
const NONEXISTENT_ACCOUNT: &str = "5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuyUpnhM";

pub async fn chain_works_at(ws_url: &str, expected_chain: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let name: String =
		client.request("system_chain", rpc_params![]).await.expect("RPC call failed");
	assert!(!name.is_empty(), "Chain name should not be empty");
	assert_eq!(name, expected_chain);
}

pub async fn name_works_at(ws_url: &str, expected_name: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let name: String = client.request("system_name", rpc_params![]).await.expect("RPC call failed");
	assert_eq!(name, expected_name);
}

pub async fn version_works_at(ws_url: &str, expected_version: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let version: String =
		client.request("system_version", rpc_params![]).await.expect("RPC call failed");
	assert_eq!(version, expected_version);
}

pub async fn health_works_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let health: SystemHealth =
		client.request("system_health", rpc_params![]).await.expect("RPC call failed");
	assert_eq!(health, SystemHealth::default());
}

pub async fn properties_returns_json_or_null_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let properties: Option<serde_json::Value> = client
		.request("system_properties", rpc_params![])
		.await
		.expect("RPC call failed");
	if let Some(props) = &properties {
		assert!(props.is_object(), "Properties should be a JSON object");
	}
}

pub async fn account_next_index_returns_nonce_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let nonce: u32 = client
		.request("system_accountNextIndex", rpc_params![ALICE_SS58])
		.await
		.expect("RPC call failed");
	assert!(nonce < u32::MAX, "Nonce should be a valid value");
}

pub async fn account_next_index_returns_zero_for_nonexistent_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let nonce: u32 = client
		.request("system_accountNextIndex", rpc_params![NONEXISTENT_ACCOUNT])
		.await
		.expect("RPC call failed");
	assert_eq!(nonce, 0, "Nonexistent account should have nonce 0");
}

pub async fn account_next_index_invalid_address_returns_error_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Result<u32, _> = client
		.request("system_accountNextIndex", rpc_params!["not_a_valid_address"])
		.await;
	assert!(result.is_err(), "Invalid SS58 address should return an error");
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonrpsee::{RpcModule, server::ServerBuilder, types::ErrorObjectOwned};

	async fn mock_ws_url() -> (String, jsonrpsee::server::ServerHandle) {
		let server =
			ServerBuilder::default().build("127.0.0.1:0").await.expect("server should bind");
		let addr = server.local_addr().expect("local addr should be available");

		let mut module = RpcModule::new(());
		module
			.register_method("system_chain", |_, _, _| "ink-node")
			.expect("register system_chain");
		module
			.register_method("system_name", |_, _, _| "pop-fork")
			.expect("register system_name");
		module
			.register_method("system_version", |_, _, _| "1.0.0")
			.expect("register system_version");
		module
			.register_method("system_health", |_, _, _| {
				serde_json::json!({
					"peers": 0,
					"isSyncing": false,
					"shouldHavePeers": false
				})
			})
			.expect("register system_health");
		module
			.register_method("system_properties", |_, _, _| {
				Some(serde_json::json!({"ss58Format":42}))
			})
			.expect("register system_properties");
		module
			.register_method("system_accountNextIndex", |params, _, _| {
				let account: String = params.one()?;
				if account == ALICE_SS58 {
					Ok(7u32)
				} else if account == NONEXISTENT_ACCOUNT {
					Ok(0u32)
				} else {
					Err(ErrorObjectOwned::owned(-32602, "Invalid SS58 address", None::<()>))
				}
			})
			.expect("register system_accountNextIndex");

		let handle = server.start(module);
		(format!("ws://{}", addr), handle)
	}

	#[tokio::test]
	async fn all_system_scenarios_work_with_mock_server() {
		let (ws_url, handle) = mock_ws_url().await;
		chain_works_at(&ws_url, "ink-node").await;
		name_works_at(&ws_url, "pop-fork").await;
		version_works_at(&ws_url, "1.0.0").await;
		health_works_at(&ws_url).await;
		properties_returns_json_or_null_at(&ws_url).await;
		account_next_index_returns_nonce_at(&ws_url).await;
		account_next_index_returns_zero_for_nonexistent_at(&ws_url).await;
		account_next_index_invalid_address_returns_error_at(&ws_url).await;
		handle.stop().expect("server should stop");
	}
}
