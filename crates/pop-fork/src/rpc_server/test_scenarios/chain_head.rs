// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

use crate::testing::TestContext;
use jsonrpsee::{
	core::client::{ClientT, SubscriptionClientT},
	rpc_params,
	ws_client::WsClientBuilder,
};

pub async fn follow_returns_subscription_and_initialized_event() {
	let ctx = TestContext::for_rpc_server().await;
	follow_returns_subscription_and_initialized_event_at(&ctx.ws_url()).await;
}

pub async fn header_returns_header_for_valid_subscription() {
	let ctx = TestContext::for_rpc_server().await;
	header_returns_header_for_valid_subscription_at(&ctx.ws_url()).await;
}

pub async fn invalid_subscription_returns_error() {
	let ctx = TestContext::for_rpc_server().await;
	invalid_subscription_returns_error_at(&ctx.ws_url()).await;
}

pub async fn follow_returns_subscription_and_initialized_event_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let mut sub = client
		.subscribe::<serde_json::Value, _>(
			"chainHead_v1_follow",
			rpc_params![false],
			"chainHead_v1_unfollow",
		)
		.await
		.expect("Subscription should succeed");

	let event = sub.next().await.expect("Should receive event").expect("Event should be valid");
	let event_type = event.get("event").and_then(|v| v.as_str());
	assert_eq!(event_type, Some("initialized"));

	let hashes = event.get("finalizedBlockHashes").and_then(|v| v.as_array());
	assert!(hashes.is_some());
	let hashes = hashes.expect("finalized hashes should be present");
	assert!(!hashes.is_empty());
	let finalized_hash = hashes[0].as_str().expect("hash should be string");
	assert!(finalized_hash.starts_with("0x"));

	let best_event =
		sub.next().await.expect("Should receive event").expect("Event should be valid");
	assert_eq!(best_event.get("event").and_then(|v| v.as_str()), Some("bestBlockChanged"));
	assert_eq!(best_event.get("bestBlockHash").and_then(|v| v.as_str()), Some(finalized_hash));
}

pub async fn header_returns_header_for_valid_subscription_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let mut sub = client
		.subscribe::<serde_json::Value, _>(
			"chainHead_v1_follow",
			rpc_params![false],
			"chainHead_v1_unfollow",
		)
		.await
		.expect("Subscription should succeed");

	let event = sub.next().await.expect("Should receive event").expect("Event should be valid");
	let hashes = event.get("finalizedBlockHashes").unwrap().as_array().unwrap();
	let block_hash = hashes[0].as_str().unwrap();
	assert!(block_hash.starts_with("0x"));
}

pub async fn invalid_subscription_returns_error_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Result<Option<String>, _> = client
		.request(
			"chainHead_v1_header",
			rpc_params![
				"invalid-sub",
				"0x0000000000000000000000000000000000000000000000000000000000000000"
			],
		)
		.await;
	assert!(result.is_err());
}

#[cfg(test)]
mod tests {
	use super::*;
	use jsonrpsee::{
		RpcModule,
		server::ServerBuilder,
		types::{ErrorObjectOwned, SubscriptionId},
	};

	async fn mock_ws_url() -> (String, jsonrpsee::server::ServerHandle) {
		let server =
			ServerBuilder::default().build("127.0.0.1:0").await.expect("server should bind");
		let addr = server.local_addr().expect("local addr should be available");

		let mut module = RpcModule::new(());
		module
			.register_subscription(
				"chainHead_v1_follow",
				"chainHead_v1_followEvent",
				"chainHead_v1_unfollow",
				|_, pending, _, _| async move {
					let sink = pending.accept().await?;
					let sub_id = match sink.subscription_id() {
						SubscriptionId::Num(n) => n.to_string(),
						SubscriptionId::Str(s) => s.to_string(),
					};
					let hash = "0x1111111111111111111111111111111111111111111111111111111111111111";
					let init = serde_json::json!({
						"event": "initialized",
						"subscription": sub_id,
						"finalizedBlockHashes": [hash],
					});
					let best = serde_json::json!({
						"event": "bestBlockChanged",
						"bestBlockHash": hash,
					});
					sink.send(jsonrpsee::SubscriptionMessage::from_json(&init)?).await?;
					sink.send(jsonrpsee::SubscriptionMessage::from_json(&best)?).await?;
					Ok(())
				},
			)
			.expect("register follow subscription");

		module
			.register_method("chainHead_v1_header", |_, _, _| {
				Err::<Option<String>, ErrorObjectOwned>(ErrorObjectOwned::owned(
					-32602,
					"Invalid subscription ID",
					None::<()>,
				))
			})
			.expect("register chainHead_v1_header");

		let handle = server.start(module);
		(format!("ws://{}", addr), handle)
	}

	#[tokio::test]
	async fn all_chain_head_scenarios_work_with_mock_server() {
		let (ws_url, handle) = mock_ws_url().await;
		follow_returns_subscription_and_initialized_event_at(&ws_url).await;
		header_returns_header_for_valid_subscription_at(&ws_url).await;
		invalid_subscription_returns_error_at(&ws_url).await;
		handle.stop().expect("server should stop");
	}
}
