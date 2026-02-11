// SPDX-License-Identifier: GPL-3.0

//! Integration tests for rpc_server author methods.

#![cfg(feature = "integration-tests")]

use jsonrpsee::{
	core::client::{ClientT, SubscriptionClientT},
	rpc_params,
	ws_client::WsClientBuilder,
};
use pop_fork::{
	Blockchain, ExecutorConfig, SignatureMockMode,
	testing::{
		TestContext, TestContextBuilder,
		accounts::{ALICE, BOB},
		constants::TRANSFER_AMOUNT,
		helpers::{account_storage_key, build_mock_signed_extrinsic_v4, decode_free_balance},
	},
};
use scale::{Compact, Encode};
use std::time::Duration;
use subxt::config::substrate::H256;

const RPC_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const SUBSCRIPTION_EVENT_TIMEOUT: Duration = Duration::from_secs(300);

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

#[tokio::test(flavor = "multi_thread")]
async fn author_submit_extrinsic_builds_block_immediately() {
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let ctx = TestContext::for_rpc_server_with_config(config).await;
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let initial_block_number = ctx.blockchain().head_number().await;

	// Get storage key for Alice
	let alice_key = account_storage_key(&ALICE);
	let alice_balance_before = ctx
		.blockchain()
		.storage(&alice_key)
		.await
		.expect("Failed to get Alice balance")
		.map(|v| decode_free_balance(&v))
		.expect("Alice should have a balance");

	// Build a transfer extrinsic using metadata for correct pallet/call indices
	let call_data = build_transfer_call_data(ctx.blockchain()).await;

	let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
	let ext_hex = format!("0x{}", hex::encode(&extrinsic));

	// Submit the extrinsic
	let hash: String = client
		.request("author_submitExtrinsic", rpc_params![ext_hex])
		.await
		.expect("RPC call failed");

	// Hash should be returned
	assert!(hash.starts_with("0x"), "Hash should start with 0x");
	assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");

	// Block should have been built immediately
	let new_block_number = ctx.blockchain().head_number().await;

	let alice_balance_after = ctx
		.blockchain()
		.storage(&alice_key)
		.await
		.expect("Failed to get Alice balance after")
		.map(|v| decode_free_balance(&v))
		.expect("Alice should still have a balance");
	assert_eq!(
		new_block_number,
		initial_block_number + 1,
		"Block should have been built immediately"
	);

	assert_ne!(alice_balance_after, alice_balance_before);
}

#[tokio::test(flavor = "multi_thread")]
async fn author_pending_extrinsics_empty_after_submit() {
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let ctx = TestContext::for_rpc_server_with_config(config).await;
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Build and submit an extrinsic using metadata for correct pallet/call indices
	let call_data = build_transfer_call_data(ctx.blockchain()).await;

	let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
	let ext_hex = format!("0x{}", hex::encode(&extrinsic));

	// Submit the extrinsic
	let _hash: String = client
		.request("author_submitExtrinsic", rpc_params![ext_hex])
		.await
		.expect("RPC call failed");

	// Check pending extrinsics - should be empty in instant mode
	let pending: Vec<String> = client
		.request("author_pendingExtrinsics", rpc_params![])
		.await
		.expect("RPC call failed");

	assert!(pending.is_empty(), "Pending extrinsics should be empty after submit in instant mode");
}

#[tokio::test(flavor = "multi_thread")]
async fn author_submit_extrinsic_returns_correct_hash() {
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let ctx = TestContext::for_rpc_server_with_config(config).await;
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Build a transfer extrinsic using metadata for correct pallet/call indices
	let call_data = build_transfer_call_data(ctx.blockchain()).await;

	let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
	let expected_hash = H256::from(sp_core::blake2_256(&extrinsic));
	let ext_hex = format!("0x{}", hex::encode(&extrinsic));

	// Submit the extrinsic
	let hash: String = client
		.request("author_submitExtrinsic", rpc_params![ext_hex])
		.await
		.expect("RPC call failed");

	// Verify the hash matches
	assert_eq!(hash, format!("0x{}", hex::encode(expected_hash.as_bytes())));
}

#[tokio::test(flavor = "multi_thread")]
async fn author_submit_extrinsic_invalid_hex() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Submit invalid hex
	let result: Result<String, _> =
		client.request("author_submitExtrinsic", rpc_params!["not_valid_hex"]).await;

	assert!(result.is_err(), "Should fail with invalid hex");
}

#[tokio::test(flavor = "multi_thread")]
async fn author_submit_and_watch_sends_lifecycle_events() {
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let ctx = TestContext::for_rpc_server_with_config(config).await;
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let initial_block_number = ctx.blockchain().head_number().await;

	// Build a valid extrinsic using metadata for correct pallet/call indices
	let call_data = build_transfer_call_data(ctx.blockchain()).await;
	let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
	let ext_hex = format!("0x{}", hex::encode(&extrinsic));

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
	let new_block_number = ctx.blockchain().head_number().await;
	assert_eq!(new_block_number, initial_block_number + 1, "Block should have been built");
}

#[tokio::test(flavor = "multi_thread")]
async fn author_submit_extrinsic_rejects_garbage_with_error_code() {
	let ctx = TestContextBuilder::new().with_server().build().await;
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Submit garbage bytes
	let garbage_hex = "0xdeadbeef";

	let result: Result<String, _> =
		client.request("author_submitExtrinsic", rpc_params![garbage_hex]).await;

	assert!(result.is_err(), "Garbage should be rejected");

	// Verify we get an error (the specific code depends on how the runtime rejects it)
	let err = result.unwrap_err();
	let err_str = err.to_string();
	// Error should indicate invalid transaction
	assert!(
		err_str.contains("1010") ||
			err_str.contains("1011") ||
			err_str.contains("invalid") ||
			err_str.contains("Invalid"),
		"Error should indicate transaction invalidity: {err_str}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn author_submit_extrinsic_does_not_build_block_on_validation_failure() {
	let ctx = TestContextBuilder::new().with_server().build().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let initial_block_number = ctx.blockchain().head_number().await;

	// Submit garbage bytes
	let garbage_hex = "0xdeadbeef";

	let _result: Result<String, _> =
		client.request("author_submitExtrinsic", rpc_params![garbage_hex]).await;

	// Block number should NOT have changed (no block built for invalid tx)
	let new_block_number = ctx.blockchain().head_number().await;
	assert_eq!(
		initial_block_number, new_block_number,
		"Block should NOT be built when extrinsic validation fails"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn author_submit_and_watch_sends_invalid_on_validation_failure() {
	let ctx = TestContextBuilder::new().with_server().build().await;
	let client = WsClientBuilder::default()
		.request_timeout(RPC_REQUEST_TIMEOUT)
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Submit garbage bytes via subscription
	let garbage_hex = "0xdeadbeef";

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
