// SPDX-License-Identifier: GPL-3.0

//! Integration tests for fork functionality.

#![cfg(all(feature = "chain", feature = "integration-tests"))]

use anyhow::Result;
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
use pop_common::{pop, test_env::InkTestNode};
use scale::{Compact, Decode, Encode};
use sp_core::{blake2_128, twox_128};
use std::time::Duration;
use subxt::Metadata;
use tokio::{process::Child, time::sleep};

/// Utility wrapper for child process cleanup.
struct TestChildProcess(Child);

impl Drop for TestChildProcess {
	fn drop(&mut self) {
		let _ = self.0.start_kill();
	}
}

/// Alice's account ID (well-known dev account).
const ALICE: [u8; 32] = [
	0xd4, 0x35, 0x93, 0xc7, 0x15, 0xfd, 0xd3, 0x1c, 0x61, 0x14, 0x1a, 0xbd, 0x04, 0xa9, 0x9f, 0xd6,
	0x82, 0x2c, 0x85, 0x58, 0x85, 0x4c, 0xcd, 0xe3, 0x9a, 0x56, 0x84, 0xe7, 0xa5, 0x6d, 0xa2, 0x7d,
];

/// Bob's account ID (well-known dev account).
const BOB: [u8; 32] = [
	0x8e, 0xaf, 0x04, 0x15, 0x16, 0x87, 0x73, 0x63, 0x26, 0xc9, 0xfe, 0xa1, 0x7e, 0x25, 0xfc, 0x52,
	0x87, 0x61, 0x36, 0x93, 0xc9, 0x12, 0x90, 0x9c, 0xb2, 0x26, 0xaa, 0x47, 0x94, 0xf2, 0x6a, 0x48,
];

/// Transfer amount: 100 units (with 12 decimals).
const TRANSFER_AMOUNT: u128 = 100_000_000_000_000;

/// Build System::Account storage key.
/// Format: twox128("System") + twox128("Account") + blake2_128_concat(account)
fn account_storage_key(account: &[u8; 32]) -> Vec<u8> {
	let mut key = Vec::new();
	key.extend(twox_128(b"System"));
	key.extend(twox_128(b"Account"));
	key.extend(blake2_128(account));
	key.extend(account);
	key
}

/// Extract free balance from AccountInfo SCALE-encoded data.
/// AccountInfo layout: nonce (4) + consumers (4) + providers (4) + sufficients (4) = 16 bytes
/// Then AccountData: free (16) + reserved (16) + ...
fn decode_free_balance(data: &[u8]) -> u128 {
	const ACCOUNT_DATA_OFFSET: usize = 16;
	u128::from_le_bytes(data[ACCOUNT_DATA_OFFSET..ACCOUNT_DATA_OFFSET + 16].try_into().unwrap())
}

/// Build a mock V4 signed extrinsic with dummy signature (from Alice).
/// Works when signature mocking is enabled (AlwaysValid mode).
fn build_mock_signed_extrinsic_v4(call_data: &[u8]) -> Vec<u8> {
	let mut inner = Vec::new();
	inner.push(0x84); // Version: signed (0x80) + v4 (0x04)
	inner.push(0x00); // MultiAddress::Id variant
	inner.extend(ALICE);
	inner.extend([0u8; 64]); // Dummy signature (works with AlwaysValid)
	inner.push(0x00); // CheckMortality: immortal
	inner.extend(Compact(0u64).encode()); // CheckNonce
	inner.extend(Compact(0u128).encode()); // ChargeTransactionPayment
	inner.push(0x00); // EthSetOrigin: None (ink-node specific)
	inner.extend(call_data);
	let mut extrinsic = Compact(inner.len() as u32).encode();
	extrinsic.extend(inner);
	extrinsic
}

/// Build call data for Balances.transfer_keep_alive using metadata.
fn build_transfer_call_data(metadata: &Metadata) -> Vec<u8> {
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

/// Integration test: Fork ink-node and perform a balance transfer.
#[tokio::test]
async fn fork_and_transfer_balance() -> Result<()> {
	let temp = tempfile::tempdir()?;
	let temp_dir = temp.path();

	// 1. Spawn ink-node as source chain
	let node = InkTestNode::spawn().await?;
	let source_url = node.ws_url();

	// 2. Launch pop fork with signature mocking
	let fork_port = 18545u16; // Use high port to avoid conflicts
	let mut fork_cmd = pop(
		temp_dir,
		["fork", "-e", source_url, "--port", &fork_port.to_string(), "--mock-all-signatures"],
	);
	let _fork_process = TestChildProcess(fork_cmd.spawn()?);

	// 3. Wait for fork server to be ready
	let fork_ws_url = format!("ws://127.0.0.1:{}", fork_port);
	let mut attempts = 0;
	let client = loop {
		sleep(Duration::from_secs(2)).await;
		match WsClientBuilder::default()
			.request_timeout(Duration::from_secs(120))
			.build(&fork_ws_url)
			.await
		{
			Ok(c) => break c,
			Err(_) => {
				attempts += 1;
				if attempts > 30 {
					panic!("Fork server did not start in time");
				}
			},
		}
	};

	// 4. Query Alice's balance before transfer
	let alice_key_hex = format!("0x{}", hex::encode(account_storage_key(&ALICE)));
	let alice_data_before: Option<String> =
		client.request("state_getStorage", rpc_params![&alice_key_hex]).await?;
	let alice_balance_before = alice_data_before
		.map(|v| decode_free_balance(&hex::decode(v.trim_start_matches("0x")).unwrap()))
		.expect("Alice should have balance");

	// 5. Query Bob's balance before transfer
	let bob_key_hex = format!("0x{}", hex::encode(account_storage_key(&BOB)));
	let bob_data_before: Option<String> =
		client.request("state_getStorage", rpc_params![&bob_key_hex]).await?;
	let bob_balance_before = bob_data_before
		.map(|v| decode_free_balance(&hex::decode(v.trim_start_matches("0x")).unwrap()))
		.expect("Bob should have balance");

	// 6. Fetch metadata to build the transfer call correctly
	let metadata_hex: String = client.request("state_getMetadata", rpc_params![]).await?;
	let metadata_bytes = hex::decode(metadata_hex.trim_start_matches("0x"))?;
	let metadata = Metadata::decode(&mut metadata_bytes.as_slice())?;

	// 7. Build and submit balance transfer extrinsic
	let call_data = build_transfer_call_data(&metadata);
	let extrinsic = build_mock_signed_extrinsic_v4(&call_data);
	let ext_hex = format!("0x{}", hex::encode(&extrinsic));

	let tx_hash: String = client
		.request("author_submitExtrinsic", rpc_params![ext_hex])
		.await
		.expect("Failed to submit extrinsic");

	// Verify we got a valid hash back
	assert!(tx_hash.starts_with("0x"), "Transaction hash should start with 0x");
	assert_eq!(tx_hash.len(), 66, "Transaction hash should be 0x + 64 hex chars");

	// 8. Query Alice's balance after transfer
	let alice_data_after: Option<String> =
		client.request("state_getStorage", rpc_params![&alice_key_hex]).await?;
	let alice_balance_after = alice_data_after
		.map(|v| decode_free_balance(&hex::decode(v.trim_start_matches("0x")).unwrap()))
		.expect("Alice should still have balance");

	// 9. Query Bob's balance after transfer
	let bob_data_after: Option<String> =
		client.request("state_getStorage", rpc_params![&bob_key_hex]).await?;
	let bob_balance_after = bob_data_after
		.map(|v| decode_free_balance(&hex::decode(v.trim_start_matches("0x")).unwrap()))
		.expect("Bob should still have balance");

	// 10. Verify the transfer happened
	assert!(
		alice_balance_after < alice_balance_before,
		"Alice's balance should decrease after transfer. Before: {}, After: {}",
		alice_balance_before,
		alice_balance_after
	);

	assert_eq!(
		bob_balance_after,
		bob_balance_before + TRANSFER_AMOUNT,
		"Bob should receive exactly the transfer amount. Before: {}, After: {}",
		bob_balance_before,
		bob_balance_after
	);

	// Verify Alice paid at least the transfer amount (plus some fees)
	assert!(
		alice_balance_before - alice_balance_after >= TRANSFER_AMOUNT,
		"Alice should have paid at least the transfer amount"
	);

	Ok(())
}
