// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

use crate::{
	rpc_server::types::RuntimeVersion,
	testing::{TestContext, accounts, helpers},
};
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

pub async fn state_get_storage_returns_value() {
	let ctx = TestContext::for_rpc_server().await;
	let key_hex = format!("0x{}", hex::encode(helpers::account_storage_key(&accounts::ALICE)));
	state_get_storage_returns_value_at(&ctx.ws_url(), &key_hex).await;
}

pub async fn state_get_storage_at_block_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let key_hex = format!("0x{}", hex::encode(helpers::account_storage_key(&accounts::ALICE)));
	let block = ctx.blockchain().build_empty_block().await.expect("block build should work");
	ctx.blockchain().build_empty_block().await.expect("block build should work");
	let block_hash_hex = format!("0x{}", hex::encode(block.hash.as_bytes()));
	state_get_storage_at_block_hash_at(&ctx.ws_url(), &key_hex, &block_hash_hex).await;
}

pub async fn state_get_storage_returns_none_for_nonexistent_key() {
	let ctx = TestContext::for_rpc_server().await;
	state_get_storage_returns_none_for_nonexistent_key_at(&ctx.ws_url()).await;
}

pub async fn state_get_metadata_returns_metadata() {
	let ctx = TestContext::for_rpc_server().await;
	state_get_metadata_returns_metadata_at(&ctx.ws_url()).await;
}

pub async fn state_get_metadata_at_block_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let head_hash_hex = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	state_get_metadata_at_block_hash_at(&ctx.ws_url(), &head_hash_hex).await;
}

pub async fn state_get_runtime_version_returns_version() {
	let ctx = TestContext::for_rpc_server().await;
	state_get_runtime_version_returns_version_at(&ctx.ws_url()).await;
}

pub async fn state_get_runtime_version_at_block_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let head_hash_hex = format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	state_get_runtime_version_at_block_hash_at(&ctx.ws_url(), &head_hash_hex).await;
}

pub async fn state_get_storage_invalid_hex_returns_error() {
	let ctx = TestContext::for_rpc_server().await;
	state_get_storage_invalid_hex_returns_error_at(&ctx.ws_url()).await;
}

pub async fn state_get_storage_invalid_block_hash_returns_error() {
	let ctx = TestContext::for_rpc_server().await;
	let key_hex = format!("0x{}", hex::encode(helpers::account_storage_key(&accounts::ALICE)));
	state_get_storage_invalid_block_hash_returns_error_at(&ctx.ws_url(), &key_hex).await;
}

pub async fn state_get_storage_returns_value_at(ws_url: &str, key_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Option<String> = client
		.request("state_getStorage", rpc_params![key_hex])
		.await
		.expect("RPC call failed");
	assert!(result.is_some(), "Key should exist");
	let value = result.expect("value expected");
	assert!(value.starts_with("0x"));
	assert!(value.len() > 2);
}

pub async fn state_get_storage_at_block_hash_at(ws_url: &str, key_hex: &str, block_hash_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Option<String> = client
		.request("state_getStorage", rpc_params![key_hex, block_hash_hex])
		.await
		.expect("RPC call failed");
	assert!(result.is_some(), "Key should exist at block");
}

pub async fn state_get_storage_returns_none_for_nonexistent_key_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let fake_key = "0x0000000000000000000000000000000000000000000000000000000000000000";
	let result: Option<String> = client
		.request("state_getStorage", rpc_params![fake_key])
		.await
		.expect("RPC call failed");
	assert!(result.is_none());
}

pub async fn state_get_metadata_returns_metadata_at(ws_url: &str) {
	let client = WsClientBuilder::default()
		.request_timeout(std::time::Duration::from_secs(120))
		.build(ws_url)
		.await
		.expect("Failed to connect");
	let result: String = client
		.request("state_getMetadata", rpc_params![])
		.await
		.expect("RPC call failed");
	assert!(result.starts_with("0x"));
	assert!(result.len() > 1000);
	assert!(result.starts_with("0x6d657461"));
}

pub async fn state_get_metadata_at_block_hash_at(ws_url: &str, head_hash_hex: &str) {
	let client = WsClientBuilder::default()
		.request_timeout(std::time::Duration::from_secs(120))
		.build(ws_url)
		.await
		.expect("Failed to connect");
	let result: String = client
		.request("state_getMetadata", rpc_params![head_hash_hex])
		.await
		.expect("RPC call failed");
	assert!(result.starts_with("0x"));
	assert!(result.len() > 1000);
}

pub async fn state_get_runtime_version_returns_version_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: RuntimeVersion = client
		.request("state_getRuntimeVersion", rpc_params![])
		.await
		.expect("RPC call failed");
	assert!(!result.spec_name.is_empty());
	assert!(!result.impl_name.is_empty());
	assert!(result.spec_version > 0);
}

pub async fn state_get_runtime_version_at_block_hash_at(ws_url: &str, head_hash_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: RuntimeVersion = client
		.request("state_getRuntimeVersion", rpc_params![head_hash_hex])
		.await
		.expect("RPC call failed");
	assert!(!result.spec_name.is_empty());
	assert!(result.spec_version > 0);
}

pub async fn state_get_storage_invalid_hex_returns_error_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Result<Option<String>, _> =
		client.request("state_getStorage", rpc_params!["not_valid_hex"]).await;
	assert!(result.is_err());
}

pub async fn state_get_storage_invalid_block_hash_returns_error_at(ws_url: &str, key_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let invalid_hash = "0x0000000000000000000000000000000000000000000000000000000000000000";
	let result: Result<Option<String>, _> =
		client.request("state_getStorage", rpc_params![key_hex, invalid_hash]).await;
	assert!(result.is_err());
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonrpsee::{RpcModule, server::ServerBuilder, types::ErrorObjectOwned};

	async fn mock_ws_url() -> (String, jsonrpsee::server::ServerHandle, String) {
		let server =
			ServerBuilder::default().build("127.0.0.1:0").await.expect("server should bind");
		let addr = server.local_addr().expect("local addr should be available");
		let key_hex = "0xaaaaaaaa".to_string();
		let valid_block_hash = format!("0x{}", "11".repeat(32));

		let mut module = RpcModule::new(());
		let key_for_method = key_hex.clone();
		let hash_for_method = valid_block_hash.clone();
		module
			.register_method("state_getStorage", move |params, _, _| {
				let values: Vec<String> = params.parse()?;
				if values.is_empty() {
					return Err(ErrorObjectOwned::owned(-32602, "missing key", None::<()>));
				}
				if values[0] == "not_valid_hex" {
					return Err(ErrorObjectOwned::owned(-32602, "invalid key", None::<()>));
				}
				if values.len() > 1 && values[1] != hash_for_method {
					return Err(ErrorObjectOwned::owned(-32602, "invalid block hash", None::<()>));
				}
				if values[0] == key_for_method {
					Ok(Some("0x01".to_string()))
				} else {
					Ok(None::<String>)
				}
			})
			.expect("register state_getStorage");

		module
			.register_method("state_getMetadata", |_, _, _| {
				format!("0x6d657461{}", "00".repeat(600))
			})
			.expect("register state_getMetadata");

		module
			.register_method("state_getRuntimeVersion", |_, _, _| {
				serde_json::json!({
					"specName": "mock-spec",
					"implName": "mock-impl",
					"authoringVersion": 1,
					"specVersion": 1,
					"implVersion": 1,
					"transactionVersion": 1,
					"stateVersion": 1,
					"apis": [],
				})
			})
			.expect("register state_getRuntimeVersion");

		let handle = server.start(module);
		(format!("ws://{}", addr), handle, valid_block_hash)
	}

	#[tokio::test]
	async fn all_state_scenarios_work_with_mock_server() {
		let (ws_url, handle, valid_block_hash) = mock_ws_url().await;
		let key_hex = "0xaaaaaaaa";
		state_get_storage_returns_value_at(&ws_url, key_hex).await;
		state_get_storage_at_block_hash_at(&ws_url, key_hex, &valid_block_hash).await;
		state_get_storage_returns_none_for_nonexistent_key_at(&ws_url).await;
		state_get_metadata_returns_metadata_at(&ws_url).await;
		state_get_metadata_at_block_hash_at(&ws_url, &valid_block_hash).await;
		state_get_runtime_version_returns_version_at(&ws_url).await;
		state_get_runtime_version_at_block_hash_at(&ws_url, &valid_block_hash).await;
		state_get_storage_invalid_hex_returns_error_at(&ws_url).await;
		state_get_storage_invalid_block_hash_returns_error_at(&ws_url, key_hex).await;
		handle.stop().expect("server should stop");
	}
}
