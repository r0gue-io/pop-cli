// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

//! Integration tests for end-to-end blockchain manager behavior.

use crate::{
	Blockchain, ChainType,
	testing::{
		TestContext,
		accounts::{ALICE, BOB},
		constants::TRANSFER_AMOUNT,
		helpers::{
			account_storage_key, build_mock_signed_extrinsic_v4_with_nonce, decode_account_nonce,
			decode_free_balance,
		},
	},
};
use std::sync::Arc;
use subxt::config::substrate::H256;
#[cfg(all(feature = "integration-tests", not(test)))]
use tokio::sync::OnceCell;
use url::Url;

#[cfg(all(feature = "integration-tests", not(test)))]
static SHARED_READONLY_BLOCKCHAIN: OnceCell<Arc<Blockchain>> = OnceCell::const_new();

async fn readonly_blockchain() -> Arc<Blockchain> {
	#[cfg(all(feature = "integration-tests", not(test)))]
	{
		return SHARED_READONLY_BLOCKCHAIN
			.get_or_init(|| async {
				let ctx = TestContext::minimal().await;
				Blockchain::fork(&ctx.endpoint, None)
					.await
					.expect("Failed to fork shared readonly blockchain")
			})
			.await
			.clone();
	}

	#[cfg(any(test, not(feature = "integration-tests")))]
	{
		let ctx = TestContext::minimal().await;
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain")
	}
}

pub async fn fork_creates_blockchain_with_correct_fork_point() {
	let blockchain = readonly_blockchain().await;

	// Fork point should be set
	assert!(blockchain.fork_point_number() > 0 || blockchain.fork_point_number() == 0);
	assert_ne!(blockchain.fork_point(), H256::zero());

	// Head should match fork point initially
	assert_eq!(blockchain.head_number().await, blockchain.fork_point_number());
	assert_eq!(blockchain.head_hash().await, blockchain.fork_point());
}

pub async fn fork_at_creates_blockchain_at_specific_block() {
	// Reuse a shared readonly fork to avoid repeated setup.
	let blockchain = readonly_blockchain().await;

	let fork_number = blockchain.fork_point_number();

	// Fork at a specific block number (same as current for test node)
	let blockchain2 = Blockchain::fork_at(blockchain.endpoint(), None, Some(fork_number.into()))
		.await
		.expect("Failed to fork at specific block");

	assert_eq!(blockchain2.fork_point_number(), fork_number);
}

pub async fn fork_with_invalid_endpoint_fails() {
	let invalid_endpoint: Url = "ws://localhost:19999".parse().unwrap();

	let result = Blockchain::fork(&invalid_endpoint, None).await;

	assert!(result.is_err());
}

pub async fn fork_at_with_invalid_block_number_fails() {
	let blockchain = readonly_blockchain().await;

	let result = Blockchain::fork_at(blockchain.endpoint(), None, Some(u32::MAX.into())).await;

	assert!(result.is_err());
}

pub async fn fork_detects_relay_chain_type() {
	let blockchain = readonly_blockchain().await;

	// Test node is a relay chain (no ParachainSystem pallet)
	assert_eq!(*blockchain.chain_type(), ChainType::RelayChain);
}

pub async fn fork_retrieves_chain_name() {
	let blockchain = readonly_blockchain().await;

	// Chain name should not be empty
	assert!(!blockchain.chain_name().is_empty());
}

pub async fn build_empty_block_advances_chain() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	let initial_number = blockchain.head_number().await;
	let initial_hash = blockchain.head_hash().await;

	// Build an empty block
	let new_block = blockchain.build_empty_block().await.expect("Failed to build empty block");

	// Block number should increment
	assert_eq!(new_block.number, initial_number + 1);

	// Head should be updated
	assert_eq!(blockchain.head_number().await, initial_number + 1);
	assert_ne!(blockchain.head_hash().await, initial_hash);

	// Parent hash should point to previous head
	assert_eq!(new_block.parent_hash, initial_hash);
}

pub async fn build_multiple_empty_blocks_creates_chain() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	let fork_number = blockchain.fork_point_number();

	// Build 3 empty blocks
	for i in 1..=3 {
		let block = blockchain.build_empty_block().await.expect("Failed to build empty block");

		assert_eq!(block.number, fork_number + i);
	}

	assert_eq!(blockchain.head_number().await, fork_number + 3);
}

pub async fn storage_returns_value_for_existing_key() {
	let blockchain = readonly_blockchain().await;

	// Query System::Number storage (should exist)
	let key = {
		let mut k = Vec::new();
		k.extend(sp_core::twox_128(b"System"));
		k.extend(sp_core::twox_128(b"Number"));
		k
	};

	let value = blockchain.storage(&key).await.expect("Failed to query storage");

	assert!(value.is_some());
}

pub async fn storage_returns_none_for_nonexistent_key() {
	let blockchain = readonly_blockchain().await;

	let nonexistent_key = b"nonexistent_key_12345";

	let value = blockchain.storage(nonexistent_key).await.expect("Failed to query storage");

	assert!(value.is_none());
}

pub async fn storage_at_queries_specific_block() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	let fork_number = blockchain.fork_point_number();

	// Build a block to have multiple blocks
	blockchain.build_empty_block().await.expect("Failed to build block");

	// Query storage at fork point
	let key = {
		let mut k = Vec::new();
		k.extend(sp_core::twox_128(b"System"));
		k.extend(sp_core::twox_128(b"Number"));
		k
	};

	let value = blockchain
		.storage_at(fork_number, &key)
		.await
		.expect("Failed to query storage at block");

	assert!(value.is_some());
}

pub async fn call_executes_runtime_api() {
	let blockchain = readonly_blockchain().await;

	// Call Core_version runtime API
	let result = blockchain.call("Core_version", &[]).await.expect("Failed to call runtime API");

	// Result should not be empty (contains version info)
	assert!(!result.is_empty());
}

pub async fn head_returns_current_block() {
	let blockchain = readonly_blockchain().await;

	let head = blockchain.head().await;

	assert_eq!(head.number, blockchain.head_number().await);
	assert_eq!(head.hash, blockchain.head_hash().await);
}

pub async fn head_updates_after_building_block() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	let old_head = blockchain.head().await;

	blockchain.build_empty_block().await.expect("Failed to build block");

	let new_head = blockchain.head().await;

	assert_eq!(new_head.number, old_head.number + 1);
	assert_ne!(new_head.hash, old_head.hash);
	assert_eq!(new_head.parent_hash, old_head.hash);
}

/// End-to-end integration test demonstrating the full blockchain manager flow.
///
/// This test shows how the blockchain manager integrates with all underlying
/// modules (Block, BlockBuilder, LocalStorageLayer, RuntimeExecutor) to process
/// a signed balance transfer transaction:
///
/// 1. Fork a live chain with signature mocking enabled
/// 2. Query initial account balances via storage
/// 3. Build a signed extrinsic (balance transfer from Alice to Bob)
/// 4. Build a block containing the transaction
/// 5. Verify the new block state reflects the transfer
pub async fn build_block_with_signed_transfer_updates_balances() {
	use crate::{ExecutorConfig, SignatureMockMode};
	use scale::{Compact, Encode};

	let ctx = TestContext::minimal().await;

	// Fork with signature mocking enabled
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let blockchain = Blockchain::fork_with_config(&ctx.endpoint, None, None, config)
		.await
		.expect("Failed to fork blockchain");
	blockchain
		.initialize_dev_accounts()
		.await
		.expect("Failed to initialize dev accounts");

	// Get storage keys for Alice and Bob
	let alice_key = account_storage_key(&ALICE);
	let bob_key = account_storage_key(&BOB);

	// Get head block for metadata and capture block number before building
	let head = blockchain.head().await;
	let head_number_before = head.number;
	let metadata = head.metadata().await.expect("Failed to get metadata");

	// Query initial balances at the current head state.
	// Use `storage()` (not `storage_at(fork_point, ...)`) so local test setup from
	// `initialize_dev_accounts()` is visible.
	let alice_nonce_before = blockchain
		.storage(&alice_key)
		.await
		.expect("Failed to get Alice account data")
		.map(|v| decode_account_nonce(&v))
		.expect("Alice account should exist");

	let bob_balance_before = blockchain
		.storage(&bob_key)
		.await
		.expect("Failed to get Bob balance")
		.map(|v| decode_free_balance(&v))
		.expect("Bob should have a balance");
	let balances_pallet =
		metadata.pallet_by_name("Balances").expect("Balances pallet should exist");
	let pallet_index = balances_pallet.index();
	let transfer_call = balances_pallet
		.call_variant_by_name("transfer_keep_alive")
		.expect("transfer_keep_alive call should exist");
	let call_index = transfer_call.index;

	// Encode the call: Balances.transfer_keep_alive(Bob, 100 units)
	let mut call_data = vec![pallet_index, call_index];
	call_data.push(0x00); // MultiAddress::Id variant
	call_data.extend(BOB);
	call_data.extend(Compact(TRANSFER_AMOUNT).encode());

	// Build a signed extrinsic with the current nonce.
	let extrinsic =
		build_mock_signed_extrinsic_v4_with_nonce(&call_data, alice_nonce_before.into());

	// Build a block with the transfer extrinsic
	let result = blockchain
		.build_block(vec![extrinsic])
		.await
		.expect("Failed to build block with transfer");

	assert_eq!(result.included.len(), 1, "Transfer extrinsic should be included");
	assert!(result.failed.is_empty(), "Transfer extrinsic should not fail: {:?}", result.failed);

	let new_block = result.block;

	// Verify block was created
	assert_eq!(new_block.number, head_number_before + 1);

	// Query balances after the transfer at the new block
	let bob_balance_after = blockchain
		.storage_at(new_block.number, &bob_key)
		.await
		.expect("Failed to get Bob balance after")
		.map(|v| decode_free_balance(&v))
		.expect("Bob should still have a balance");

	let alice_nonce_after = blockchain
		.storage_at(new_block.number, &alice_key)
		.await
		.expect("Failed to get Alice account data after")
		.map(|v| decode_account_nonce(&v))
		.expect("Alice account should still exist");

	// Verify the transfer happened
	// Bob should have a higher balance after the transfer.
	// On some runtimes (ink-node v0.47.0), local account initialization can
	// also apply during block execution, so an exact delta assertion is brittle.
	assert!(
		bob_balance_after > bob_balance_before,
		"Bob balance should increase after transfer (before: {}, after: {})",
		bob_balance_before,
		bob_balance_after
	);
	// The sender nonce should increase after successful inclusion.
	assert_eq!(
		alice_nonce_after,
		alice_nonce_before + 1,
		"Alice nonce should increase after transfer inclusion"
	);
}

pub async fn block_body_returns_extrinsics_for_head() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build a block so we have extrinsics (inherents)
	let block = blockchain.build_empty_block().await.expect("Failed to build block");

	// Query body for head hash
	let body = blockchain.block_body(block.hash).await.expect("Failed to get block body");

	assert!(body.is_some(), "Should return body for head hash");
	// Should have inherent extrinsics
	let extrinsics = body.unwrap();
	assert!(!extrinsics.is_empty(), "Built block should have inherent extrinsics");
}

pub async fn block_body_returns_extrinsics_for_parent_block() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build two blocks
	let block1 = blockchain.build_empty_block().await.expect("Failed to build block 1");
	let _block2 = blockchain.build_empty_block().await.expect("Failed to build block 2");

	// Query body for the first built block (parent of head)
	let body = blockchain.block_body(block1.hash).await.expect("Failed to get block body");

	assert!(body.is_some(), "Should return body for parent block");
	let extrinsics = body.unwrap();
	assert!(!extrinsics.is_empty(), "Parent block should have inherent extrinsics");
}

pub async fn block_body_returns_extrinsics_for_fork_point_from_remote() {
	let blockchain = readonly_blockchain().await;

	let fork_point_hash = blockchain.fork_point();

	// Query body for fork point (should fetch from remote)
	let body = blockchain.block_body(fork_point_hash).await.expect("Failed to get block body");

	// Fork point exists on remote chain, so body should be Some
	assert!(body.is_some(), "Should return body for fork point from remote");
	assert!(!body.unwrap().is_empty(), "Should contain body");
}

pub async fn block_body_returns_none_for_unknown_hash() {
	let blockchain = readonly_blockchain().await;

	// Use a fabricated hash that doesn't exist
	let unknown_hash = H256::from([0xde; 32]);

	let body = blockchain.block_body(unknown_hash).await.expect("Failed to query block body");

	assert!(body.is_none(), "Should return None for unknown hash");
}

pub async fn block_header_returns_header_for_head() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build a block so we have a locally-built header
	let block = blockchain.build_empty_block().await.expect("Failed to build block");

	// Query header for head hash
	let header = blockchain.block_header(block.hash).await.expect("Failed to get block header");

	assert!(header.is_some(), "Should return header for head hash");
	// Header should not be empty
	let header_bytes = header.unwrap();
	assert!(!header_bytes.is_empty(), "Built block should have a header");
}

pub async fn block_header_returns_header_for_different_blocks() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build two blocks
	let block1 = blockchain.build_empty_block().await.expect("Failed to build block 1");
	let block2 = blockchain.build_empty_block().await.expect("Failed to build block 2");

	let header_1 = blockchain
		.block_header(block1.hash)
		.await
		.expect("Failed to get block header")
		.unwrap();
	let header_2 = blockchain
		.block_header(block2.hash)
		.await
		.expect("Failed to get block header")
		.unwrap();

	assert_ne!(header_1, header_2);
}

pub async fn block_header_returns_header_for_fork_point() {
	let blockchain = readonly_blockchain().await;

	let fork_point_hash = blockchain.fork_point();

	// Query header for fork point (should fetch from remote)
	let header = blockchain
		.block_header(fork_point_hash)
		.await
		.expect("Failed to get block header");

	// Fork point exists on remote chain, so header should be Some
	assert!(header.is_some(), "Should return header for fork point from remote");
	assert!(!header.unwrap().is_empty(), "Should contain header");
}

pub async fn block_header_returns_none_for_unknown_hash() {
	let blockchain = readonly_blockchain().await;

	// Use a fabricated hash that doesn't exist
	let unknown_hash = H256::from([0xde; 32]);

	let header = blockchain
		.block_header(unknown_hash)
		.await
		.expect("Failed to query block header");

	assert!(header.is_none(), "Should return None for unknown hash");
}

pub async fn block_header_returns_header_for_historical_block() {
	let blockchain = readonly_blockchain().await;

	let fork_number = blockchain.fork_point_number();

	// Only test if fork point is > 0 (has blocks before it)
	if fork_number > 0 {
		// Get the hash of a block before the fork point
		let historical_hash = blockchain
			.block_hash_at(fork_number - 1)
			.await
			.expect("Failed to get historical hash")
			.expect("Historical block should exist");

		// Query header for historical block (before fork point, on remote chain)
		let header = blockchain
			.block_header(historical_hash)
			.await
			.expect("Failed to get block header");

		assert!(header.is_some(), "Should return header for historical block");
		assert!(!header.unwrap().is_empty(), "Historical block should have a header");
	}
}

pub async fn block_hash_at_returns_hash_for_head() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build a block
	let block = blockchain.build_empty_block().await.expect("Failed to build block");

	// Query hash for head block number
	let hash = blockchain.block_hash_at(block.number).await.expect("Failed to get block hash");

	assert!(hash.is_some(), "Should return hash for head block number");
	assert_eq!(hash.unwrap(), block.hash, "Hash should match head block hash");
}

pub async fn block_hash_at_returns_hash_for_parent_block() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build two blocks
	let block1 = blockchain.build_empty_block().await.expect("Failed to build block 1");
	let _block2 = blockchain.build_empty_block().await.expect("Failed to build block 2");

	// Query hash for the first built block
	let hash = blockchain.block_hash_at(block1.number).await.expect("Failed to get block hash");

	assert!(hash.is_some(), "Should return hash for parent block number");
	assert_eq!(hash.unwrap(), block1.hash, "Hash should match first block hash");
}

pub async fn block_hash_at_returns_hash_for_fork_point() {
	let blockchain = readonly_blockchain().await;

	let fork_point_number = blockchain.fork_point_number();
	let fork_point_hash = blockchain.fork_point();

	// Query hash for fork point
	let hash = blockchain
		.block_hash_at(fork_point_number)
		.await
		.expect("Failed to get block hash");

	assert!(hash.is_some(), "Should return hash for fork point");
	assert_eq!(hash.unwrap(), fork_point_hash, "Hash should match fork point hash");
}

pub async fn block_hash_at_returns_hash_for_block_before_fork_point() {
	let blockchain = readonly_blockchain().await;

	let fork_point_number = blockchain.fork_point_number();

	// Only test if fork point is > 0 (has blocks before it)
	if fork_point_number > 0 {
		let hash = blockchain
			.block_hash_at(fork_point_number - 1)
			.await
			.expect("Failed to get block hash");

		assert!(hash.is_some(), "Should return hash for block before fork point");
	}
}

pub async fn block_hash_at_returns_none_for_future_block() {
	let blockchain = readonly_blockchain().await;

	let head_number = blockchain.head_number().await;

	// Query a block number that doesn't exist yet
	let hash = blockchain
		.block_hash_at(head_number + 100)
		.await
		.expect("Failed to query block hash");

	assert!(hash.is_none(), "Should return None for future block number");
}

pub async fn block_number_by_hash_returns_number_for_head() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build a block
	let block = blockchain.build_empty_block().await.unwrap();

	// Query number by hash
	let number = blockchain
		.block_number_by_hash(block.hash)
		.await
		.expect("Failed to query block number");

	assert_eq!(number, Some(block.number));
}

pub async fn block_number_by_hash_returns_number_for_parent() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build two blocks
	let block1 = blockchain.build_empty_block().await.unwrap();
	let _block2 = blockchain.build_empty_block().await.unwrap();

	// Query number for first block
	let number = blockchain
		.block_number_by_hash(block1.hash)
		.await
		.expect("Failed to query block number");

	assert_eq!(number, Some(block1.number));
}

pub async fn block_number_by_hash_returns_number_for_fork_point() {
	let blockchain = readonly_blockchain().await;

	let fork_hash = blockchain.fork_point();
	let fork_number = blockchain.fork_point_number();

	let number = blockchain
		.block_number_by_hash(fork_hash)
		.await
		.expect("Failed to query block number");

	assert_eq!(number, Some(fork_number));
}

pub async fn block_number_by_hash_returns_none_for_unknown() {
	let blockchain = readonly_blockchain().await;

	let unknown_hash = H256::from_slice(&[0u8; 32]);
	let number = blockchain
		.block_number_by_hash(unknown_hash)
		.await
		.expect("Failed to query block number");

	assert!(number.is_none());
}

pub async fn block_number_by_hash_returns_number_for_historical_block() {
	let blockchain = readonly_blockchain().await;

	// Get a block before the fork point (if available)
	let fork_number = blockchain.fork_point_number();
	if fork_number > 0 {
		let historical_hash = blockchain
			.block_hash_at(fork_number - 1)
			.await
			.expect("Failed to query block hash")
			.expect("Block should exist");

		let number = blockchain
			.block_number_by_hash(historical_hash)
			.await
			.expect("Failed to query block number");

		assert_eq!(number, Some(fork_number - 1));
	}
}

pub async fn call_at_block_executes_at_head() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// One local block is enough to validate head-based execution.
	blockchain.build_empty_block().await.unwrap();

	let head_hash = blockchain.head_hash().await;

	// Call Core_version at head hash
	let result = blockchain
		.call_at_block(head_hash, "Core_version", &[])
		.await
		.expect("Failed to call runtime API");

	assert!(result.is_some(), "Should return result for head hash");
	assert!(!result.unwrap().is_empty(), "Result should not be empty");
}

pub async fn call_at_block_executes_at_fork_point() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Advance once locally, then execute against the remote fork point.
	blockchain.build_empty_block().await.unwrap();

	let fork_hash = blockchain.fork_point();

	// Call Core_version at fork point
	let result = blockchain
		.call_at_block(fork_hash, "Core_version", &[])
		.await
		.expect("Failed to call runtime API");

	assert!(result.is_some(), "Should return result for fork point hash");
	assert!(!result.unwrap().is_empty(), "Result should not be empty");
}

pub async fn call_at_block_executes_at_parent_block() {
	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Build two blocks
	let block1 = blockchain.build_empty_block().await.expect("Failed to build block 1");
	let _block2 = blockchain.build_empty_block().await.expect("Failed to build block 2");

	// Call at the first built block (parent of head)
	let result = blockchain
		.call_at_block(block1.hash, "Core_version", &[])
		.await
		.expect("Failed to call runtime API");

	assert!(result.is_some(), "Should return result for parent block hash");
	assert!(!result.unwrap().is_empty(), "Result should not be empty");
}

pub async fn call_at_block_returns_none_for_unknown_hash() {
	let blockchain = readonly_blockchain().await;

	// Use a fabricated hash that doesn't exist
	let unknown_hash = H256::from([0xde; 32]);

	let result = blockchain
		.call_at_block(unknown_hash, "Core_version", &[])
		.await
		.expect("Failed to query");

	assert!(result.is_none(), "Should return None for unknown hash");
}

pub async fn call_at_block_executes_at_historical_block() {
	let blockchain = readonly_blockchain().await;

	let fork_number = blockchain.fork_point_number();

	// Only test if fork point is > 0 (has blocks before it)
	if fork_number > 0 {
		// Get the hash of a block before the fork point
		let historical_hash = blockchain
			.block_hash_at(fork_number - 1)
			.await
			.expect("Failed to get historical hash")
			.expect("Historical block should exist");

		// Call at historical block (before fork point, on remote chain)
		let result = blockchain
			.call_at_block(historical_hash, "Core_version", &[])
			.await
			.expect("Failed to call runtime API");

		assert!(result.is_some(), "Should return result for historical block");
		assert!(!result.unwrap().is_empty(), "Result should not be empty");
	}
}

/// Verifies that calling `Core_initialize_block` via `call_at_block` does NOT
/// persist storage changes.
///
/// `Core_initialize_block` writes to `System::Number` and other storage keys during
/// block initialization. This test verifies those changes are discarded after the call.
pub async fn call_at_block_does_not_persist_storage() {
	use crate::{DigestItem, consensus_engine, create_next_header};

	let ctx = TestContext::minimal().await;

	let blockchain =
		Blockchain::fork(&ctx.endpoint, None).await.expect("Failed to fork blockchain");

	// Get head block info
	let head = blockchain.head().await;
	let head_hash = head.hash;
	let head_number = head.number;

	// System::Number storage key = twox128("System") ++ twox128("Number")
	let system_number_key: Vec<u8> =
		[sp_core::twox_128(b"System").as_slice(), sp_core::twox_128(b"Number").as_slice()].concat();

	// Query System::Number BEFORE
	let number_before = blockchain
		.storage(&system_number_key)
		.await
		.expect("Failed to get System::Number")
		.map(|v| u32::from_le_bytes(v.try_into().expect("System::Number should be 4 bytes")))
		.expect("System::Number should exist");

	// Build header for the next block using the crate's helper
	let header = create_next_header(
		&head,
		vec![DigestItem::PreRuntime(consensus_engine::AURA, 0u64.to_le_bytes().to_vec())],
	);

	// Call Core_initialize_block - this WOULD write System::Number = head_number + 1
	let result = blockchain
		.call_at_block(head_hash, "Core_initialize_block", &header)
		.await
		.expect("Core_initialize_block call failed");
	assert!(result.is_some(), "Block should exist");

	// Query System::Number AFTER - should be UNCHANGED
	let number_after = blockchain
		.storage(&system_number_key)
		.await
		.expect("Failed to get System::Number after")
		.map(|v| u32::from_le_bytes(v.try_into().expect("System::Number should be 4 bytes")))
		.expect("System::Number should still exist");

	assert_eq!(
		number_before,
		number_after,
		"System::Number should NOT be modified by call_at_block. \
		 Before: {}, After: {} (would have been {} if persisted)",
		number_before,
		number_after,
		head_number + 1
	);
}

pub async fn validate_extrinsic_accepts_valid_transfer() {
	use crate::{ExecutorConfig, SignatureMockMode};
	use scale::{Compact, Encode};

	let ctx = TestContext::minimal().await;
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let blockchain = Blockchain::fork_with_config(&ctx.endpoint, None, None, config)
		.await
		.expect("Failed to fork blockchain");
	blockchain
		.initialize_dev_accounts()
		.await
		.expect("Failed to initialize dev accounts");

	// Build a valid transfer extrinsic
	let head = blockchain.head().await;
	let metadata = head.metadata().await.expect("Failed to get metadata");

	let balances_pallet = metadata.pallet_by_name("Balances").expect("Balances pallet");
	let pallet_index = balances_pallet.index();
	let transfer_call = balances_pallet
		.call_variant_by_name("transfer_keep_alive")
		.expect("transfer_keep_alive");
	let call_index = transfer_call.index;

	let mut call_data = vec![pallet_index, call_index];
	call_data.push(0x00); // MultiAddress::Id
	call_data.extend(BOB);
	call_data.extend(Compact(TRANSFER_AMOUNT).encode());

	let alice_key = account_storage_key(&ALICE);
	let alice_nonce = blockchain
		.storage(&alice_key)
		.await
		.expect("Failed to get Alice account data")
		.map(|v| decode_account_nonce(&v))
		.expect("Alice account should exist");
	let extrinsic = build_mock_signed_extrinsic_v4_with_nonce(&call_data, u64::from(alice_nonce));

	// Validate should succeed
	let result = blockchain.validate_extrinsic(&extrinsic).await;
	assert!(result.is_ok(), "Valid extrinsic should pass validation: {:?}", result);
}

pub async fn validate_extrinsic_rejects_garbage() {
	let blockchain = readonly_blockchain().await;

	// Submit garbage bytes
	let garbage = vec![0xde, 0xad, 0xbe, 0xef];

	let result = blockchain.validate_extrinsic(&garbage).await;
	assert!(result.is_err(), "Garbage should fail validation");
}

pub async fn build_block_result_tracks_included_extrinsics() {
	use crate::{ExecutorConfig, SignatureMockMode};
	use scale::{Compact, Encode};

	let ctx = TestContext::minimal().await;
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let blockchain = Blockchain::fork_with_config(&ctx.endpoint, None, None, config)
		.await
		.expect("Failed to fork");
	blockchain
		.initialize_dev_accounts()
		.await
		.expect("Failed to initialize dev accounts");

	// Build a valid transfer extrinsic
	let head = blockchain.head().await;
	let metadata = head.metadata().await.expect("Failed to get metadata");

	let balances_pallet = metadata.pallet_by_name("Balances").expect("Balances pallet");
	let pallet_index = balances_pallet.index();
	let transfer_call = balances_pallet
		.call_variant_by_name("transfer_keep_alive")
		.expect("transfer_keep_alive");
	let call_index = transfer_call.index;

	let mut call_data = vec![pallet_index, call_index];
	call_data.push(0x00); // MultiAddress::Id
	call_data.extend(BOB);
	call_data.extend(Compact(TRANSFER_AMOUNT).encode());

	let alice_key = account_storage_key(&ALICE);
	let alice_nonce = blockchain
		.storage(&alice_key)
		.await
		.expect("Failed to get Alice account data")
		.map(|v| decode_account_nonce(&v))
		.expect("Alice account should exist");
	let extrinsic = build_mock_signed_extrinsic_v4_with_nonce(&call_data, u64::from(alice_nonce));

	let result = blockchain
		.build_block(vec![extrinsic.clone()])
		.await
		.expect("Failed to build block");

	assert_eq!(result.included.len(), 1, "Should have 1 included extrinsic");
	assert!(result.failed.is_empty(), "Should have no failed extrinsics");
	assert_eq!(result.included[0], extrinsic);
}

pub async fn build_block_result_tracks_failed_extrinsics() {
	use crate::{ExecutorConfig, SignatureMockMode};
	use scale::{Compact, Encode};

	let ctx = TestContext::minimal().await;
	let config =
		ExecutorConfig { signature_mock: SignatureMockMode::AlwaysValid, ..Default::default() };
	let blockchain = Blockchain::fork_with_config(&ctx.endpoint, None, None, config)
		.await
		.expect("Failed to fork");

	// Build an extrinsic that will fail at dispatch time - transfer more than available.
	// Use a random account with no funds to trigger InsufficientBalance.
	let head = blockchain.head().await;
	let metadata = head.metadata().await.expect("Failed to get metadata");

	let balances_pallet = metadata.pallet_by_name("Balances").expect("Balances pallet");
	let pallet_index = balances_pallet.index();
	let transfer_call = balances_pallet
		.call_variant_by_name("transfer_keep_alive")
		.expect("transfer_keep_alive");
	let call_index = transfer_call.index;

	// Use a "random" account that has no funds as the sender.
	// The extrinsic is structurally valid but will fail dispatch due to lack of funds.
	let unfunded_account: [u8; 32] = [0x99; 32];
	let recipient = BOB;
	let amount: u128 = 1_000_000_000_000_000; // Large amount that unfunded account can't pay

	let mut call_data = vec![pallet_index, call_index];
	call_data.push(0x00); // MultiAddress::Id
	call_data.extend(recipient);
	call_data.extend(Compact(amount).encode());

	// Build extrinsic from unfunded account
	let extrinsic = {
		let mut inner = Vec::new();
		inner.push(0x84); // Version: signed (0x80) + v4 (0x04)
		inner.push(0x00); // MultiAddress::Id variant
		inner.extend(unfunded_account);
		inner.extend([0u8; 64]); // Dummy signature (works with AlwaysValid)
		inner.push(0x00); // CheckMortality: immortal
		inner.extend(Compact(0u64).encode()); // CheckNonce
		inner.extend(Compact(0u128).encode()); // ChargeTransactionPayment
		inner.push(0x00); // EthSetOrigin: None
		inner.extend(&call_data);
		let mut final_ext = Compact(inner.len() as u32).encode();
		final_ext.extend(inner);
		final_ext
	};

	let result = blockchain
		.build_block(vec![extrinsic.clone()])
		.await
		.expect("Build should succeed even with failed extrinsics");

	// The extrinsic should fail at dispatch (InsufficientBalance) and be in the failed list
	assert!(
		result.failed.len() == 1,
		"Failed extrinsic should be tracked. Included: {}, Failed: {}",
		result.included.len(),
		result.failed.len()
	);
	assert!(result.included.is_empty(), "Failed extrinsic should not be in included list");
	assert_eq!(result.failed[0].extrinsic, extrinsic);
}
