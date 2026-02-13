// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

//! Integration tests for rpc_server author methods.

use crate::{
	Blockchain, ExecutorConfig, SignatureMockMode,
	testing::{
		TestContext, TestContextBuilder,
		accounts::{ALICE, BOB},
		constants::TRANSFER_AMOUNT,
		helpers::{
			account_storage_key, build_mock_signed_extrinsic_v4,
			build_mock_signed_extrinsic_v4_with_nonce, decode_account_nonce,
		},
	},
};
use jsonrpsee::{
	core::client::{ClientT, SubscriptionClientT},
	rpc_params,
	ws_client::WsClientBuilder,
};
use scale::{Compact, Encode};
use std::{future::Future, time::Duration};

const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(400);
const SUBSCRIPTION_EVENT_TIMEOUT: Duration = Duration::from_secs(400);
const RPC_TIMEOUT_RETRY_BACKOFF: Duration = Duration::from_secs(2);

fn is_request_timeout(err: &jsonrpsee::core::client::Error) -> bool {
	let msg = err.to_string();
	msg.contains("RequestTimeout") || msg.contains("request timeout")
}

async fn request_with_timeout_retry<T, F, Fut>(
	mut request: F,
) -> Result<T, jsonrpsee::core::client::Error>
where
	F: FnMut() -> Fut,
	Fut: Future<Output = Result<T, jsonrpsee::core::client::Error>>,
{
	for attempt in 1..=2 {
		match request().await {
			Ok(value) => return Ok(value),
			Err(err) if attempt == 1 && is_request_timeout(&err) => {
				tokio::time::sleep(RPC_TIMEOUT_RETRY_BACKOFF).await;
			},
			Err(err) => return Err(err),
		}
	}

	unreachable!("request retry loop should always return")
}

async fn author_context_with_dev_accounts() -> TestContext {
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let ctx = TestContext::for_rpc_server_with_config(config).await;
	ctx.blockchain()
		.initialize_dev_accounts()
		.await
		.expect("Failed to initialize dev accounts");
	ctx
}

async fn alice_nonce(blockchain: &Blockchain) -> u64 {
	let alice_key = account_storage_key(&ALICE);
	blockchain
		.storage(&alice_key)
		.await
		.expect("Failed to read Alice account")
		.map(|v| decode_account_nonce(&v))
		.unwrap_or(0) as u64
}

pub async fn author_submit_extrinsic_returns_correct_hash() {
	let ctx = author_context_with_dev_accounts().await;
	let base_nonce = alice_nonce(ctx.blockchain()).await;
	let ext_hex = build_transfer_extrinsic_hex_with_nonce(ctx.blockchain(), base_nonce).await;
	let expected_hash = format!(
		"0x{}",
		hex::encode(sp_core::blake2_256(
			&hex::decode(ext_hex.trim_start_matches("0x")).expect("extrinsic hex should decode")
		))
	);
	author_submit_extrinsic_returns_correct_hash_at(&ctx.ws_url(), &ext_hex, &expected_hash).await;
}

pub async fn author_pending_extrinsics_empty_after_submit() {
	let ctx = author_context_with_dev_accounts().await;
	let base_nonce = alice_nonce(ctx.blockchain()).await;
	let ext_hex = build_transfer_extrinsic_hex_with_nonce(ctx.blockchain(), base_nonce).await;
	author_pending_extrinsics_empty_after_submit_at(&ctx.ws_url(), &ext_hex).await;
}

pub async fn author_submit_extrinsic_invalid_hex() {
	let ctx = TestContext::for_rpc_server().await;
	author_submit_extrinsic_invalid_hex_at(&ctx.ws_url()).await;
}

pub async fn author_submit_extrinsic_rejects_garbage_with_error_code() {
	let ctx = TestContextBuilder::new().with_server().build().await;
	author_submit_extrinsic_rejects_garbage_with_error_code_at(&ctx.ws_url(), "0xdeadbeef").await;
}

pub async fn author_submit_and_watch_sends_lifecycle_events() {
	let ctx = author_context_with_dev_accounts().await;
	let base_nonce = alice_nonce(ctx.blockchain()).await;
	let ext_hex = build_transfer_extrinsic_hex_with_nonce(ctx.blockchain(), base_nonce).await;
	author_submit_and_watch_sends_lifecycle_events_at(&ctx.ws_url(), &ext_hex).await;
}

pub async fn author_submit_extrinsic_does_not_build_block_on_validation_failure() {
	let ctx = TestContextBuilder::new().with_server().build().await;
	author_submit_extrinsic_does_not_build_block_on_validation_failure_at(
		&ctx.ws_url(),
		"0xdeadbeef",
	)
	.await;
}

pub async fn author_submit_and_watch_sends_invalid_on_validation_failure() {
	let ctx = TestContextBuilder::new().with_server().build().await;
	author_submit_and_watch_sends_invalid_on_validation_failure_at(&ctx.ws_url(), "0xdeadbeef")
		.await;
}

pub async fn author_submit_extrinsic_returns_correct_hash_at(
	ws_url: &str,
	ext_hex: &str,
	expected_hash_hex: &str,
) {
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(ws_url)
		.await
		.expect("Failed to connect");
	let hash: String = client
		.request("author_submitExtrinsic", rpc_params![ext_hex])
		.await
		.expect("RPC call failed");
	assert_eq!(hash, expected_hash_hex);
}

pub async fn author_pending_extrinsics_empty_after_submit_at(ws_url: &str, ext_hex: &str) {
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(ws_url)
		.await
		.expect("Failed to connect");
	let _hash: String = request_with_timeout_retry(|| {
		client.request("author_submitExtrinsic", rpc_params![ext_hex])
	})
	.await
	.expect("RPC call failed");
	let pending: Vec<String> =
		request_with_timeout_retry(|| client.request("author_pendingExtrinsics", rpc_params![]))
			.await
			.expect("RPC call failed");
	assert!(pending.is_empty(), "Pending extrinsics should be empty after submit in instant mode");
}

pub async fn author_submit_extrinsic_invalid_hex_at(ws_url: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");
	let result: Result<String, _> =
		client.request("author_submitExtrinsic", rpc_params!["not_valid_hex"]).await;
	assert!(result.is_err(), "Should fail with invalid hex");
}

pub async fn author_submit_extrinsic_rejects_garbage_with_error_code_at(
	ws_url: &str,
	garbage_hex: &str,
) {
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(ws_url)
		.await
		.expect("Failed to connect");
	let result: Result<String, _> =
		client.request("author_submitExtrinsic", rpc_params![garbage_hex]).await;
	assert!(result.is_err(), "Garbage should be rejected");
	let err = result.expect_err("error expected");
	let err_str = err.to_string();
	assert!(
		err_str.contains("1010") ||
			err_str.contains("1011") ||
			err_str.contains("invalid") ||
			err_str.contains("Invalid"),
		"Error should indicate transaction invalidity: {err_str}"
	);
}

/// Build call data for Balances.transfer_keep_alive using metadata.
async fn build_transfer_call_data(blockchain: &Blockchain) -> Vec<u8> {
	let head = blockchain.head().await;
	let metadata = head.metadata().await.expect("Failed to get metadata");

	let balances_pallet =
		metadata.pallet_by_name("Balances").expect("Balances pallet should exist");
	let pallet_index = balances_pallet.index();
	let transfer_call = balances_pallet
		.call_variant_by_name("transfer_keep_alive")
		.expect("transfer_keep_alive call should exist");
	let call_index = transfer_call.index;

	let mut call_data = vec![pallet_index, call_index];
	call_data.push(0x00); // MultiAddress::Id variant
	call_data.extend(BOB);
	call_data.extend(Compact(TRANSFER_AMOUNT).encode());
	call_data
}

/// Build a valid transfer extrinsic hex string for author scenario setup.
pub async fn build_transfer_extrinsic_hex(blockchain: &Blockchain) -> String {
	build_transfer_extrinsic_hex_with_nonce(blockchain, 0).await
}

/// Build a valid transfer extrinsic hex string for author scenario setup with an explicit nonce.
pub async fn build_transfer_extrinsic_hex_with_nonce(
	blockchain: &Blockchain,
	nonce: u64,
) -> String {
	let call_data = build_transfer_call_data(blockchain).await;
	let extrinsic = if nonce == 0 {
		build_mock_signed_extrinsic_v4(&call_data)
	} else {
		build_mock_signed_extrinsic_v4_with_nonce(&call_data, nonce)
	};
	format!("0x{}", hex::encode(&extrinsic))
}

async fn chain_head_number(client: &jsonrpsee::ws_client::WsClient) -> u32 {
	let header: serde_json::Value =
		request_with_timeout_retry(|| client.request("chain_getHeader", rpc_params![]))
			.await
			.expect("chain_getHeader should succeed");
	let number_hex = header
		.get("number")
		.and_then(|v| v.as_str())
		.expect("header should contain number");
	u32::from_str_radix(number_hex.trim_start_matches("0x"), 16)
		.expect("header number should be valid hex")
}

pub async fn author_submit_and_watch_sends_lifecycle_events_at(ws_url: &str, ext_hex: &str) {
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(ws_url)
		.await
		.expect("Failed to connect");

	let initial_block_number = chain_head_number(&client).await;

	// Subscribe to extrinsic status
	let mut subscription: jsonrpsee::core::client::Subscription<serde_json::Value> = client
		.subscribe(
			"author_submitAndWatchExtrinsic",
			rpc_params![ext_hex],
			"author_unwatchExtrinsic",
		)
		.await
		.expect("Failed to subscribe");

	// Collect events with timeout.
	// Block building requires WASM compilation on first call, so allow enough time.
	let mut events = Vec::new();

	loop {
		match tokio::time::timeout(SUBSCRIPTION_EVENT_TIMEOUT, subscription.next()).await {
			Ok(Some(Ok(event))) => {
				let is_finalized = event.get("finalized").is_some();
				events.push(event);
				if is_finalized {
					break;
				}
			},
			Ok(Some(Err(e))) => panic!("Subscription error: {e}"),
			Ok(None) => break, // Stream ended
			Err(_) => panic!("Timeout waiting for subscription events"),
		}
	}

	// Verify we got the expected lifecycle events
	assert!(
		events.len() >= 3,
		"Should receive at least ready, inBlock, finalized events. Got: {:?}",
		events
	);

	// First event should be "ready"
	assert!(
		events[0].get("ready").is_some(),
		"First event should be 'ready', got: {:?}",
		events[0]
	);

	// Should have "broadcast"
	assert!(events.iter().any(|e| e.get("broadcast").is_some()), "Should have 'broadcast' event");

	// Should have "inBlock"
	let in_block_event = events.iter().find(|e| e.get("inBlock").is_some());
	assert!(in_block_event.is_some(), "Should have 'inBlock' event");

	// Should have "finalized"
	let finalized_event = events.iter().find(|e| e.get("finalized").is_some());
	assert!(finalized_event.is_some(), "Should have 'finalized' event");

	// Block should have been built
	let new_block_number = chain_head_number(&client).await;
	assert_eq!(new_block_number, initial_block_number + 1, "Block should have been built");
}

pub async fn author_submit_extrinsic_does_not_build_block_on_validation_failure_at(
	ws_url: &str,
	garbage_hex: &str,
) {
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(ws_url)
		.await
		.expect("Failed to connect");

	let initial_block_number = chain_head_number(&client).await;

	let _result: Result<String, _> =
		client.request("author_submitExtrinsic", rpc_params![garbage_hex]).await;

	// Block number should NOT have changed (no block built for invalid tx)
	let new_block_number = chain_head_number(&client).await;
	assert_eq!(
		initial_block_number, new_block_number,
		"Block should NOT be built when extrinsic validation fails"
	);
}

pub async fn author_submit_and_watch_sends_invalid_on_validation_failure_at(
	ws_url: &str,
	garbage_hex: &str,
) {
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(ws_url)
		.await
		.expect("Failed to connect");

	let mut subscription: jsonrpsee::core::client::Subscription<serde_json::Value> = client
		.subscribe(
			"author_submitAndWatchExtrinsic",
			rpc_params![garbage_hex],
			"author_unwatchExtrinsic",
		)
		.await
		.expect("Failed to subscribe");

	// Should receive "invalid" event (not "ready").
	// Validation requires WASM compilation on first call, so allow enough time.
	match tokio::time::timeout(SUBSCRIPTION_EVENT_TIMEOUT, subscription.next()).await {
		Ok(Some(Ok(event))) => {
			assert!(
				event.get("invalid").is_some(),
				"First event should be 'invalid' for garbage input, got: {:?}",
				event
			);
		},
		Ok(Some(Err(e))) => panic!("Subscription error: {e}"),
		Ok(None) => panic!("Subscription ended without events"),
		Err(_) => panic!("Timeout waiting for subscription event"),
	}
}
