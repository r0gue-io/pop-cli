// SPDX-License-Identifier: GPL-3.0

//! Integration tests for rpc_server chain methods.

#![cfg(feature = "integration-tests")]

use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
use pop_fork::{
	rpc_server::types::{RpcHeader, SignedBlock},
	testing::TestContext,
};

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_block_hash_returns_head_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Build a block so we have something beyond fork point
	ctx.blockchain().build_empty_block().await.expect("Failed to build block");

	let head_number = ctx.blockchain().head_number().await;
	let expected_hash = ctx.blockchain().head_hash().await;

	// Query with explicit block number
	let hash: Option<String> = client
		.request("chain_getBlockHash", rpc_params![head_number])
		.await
		.expect("RPC call failed");

	assert!(hash.is_some(), "Should return hash for head block");
	let hash = hash.unwrap();
	assert!(hash.starts_with("0x"), "Hash should start with 0x");
	assert_eq!(hash, format!("0x{}", hex::encode(expected_hash.as_bytes())));
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_block_hash_returns_none_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let expected_hash = ctx.blockchain().head_hash().await;

	// Query without block number (should return head)
	let hash: Option<String> = client
		.request("chain_getBlockHash", rpc_params![])
		.await
		.expect("RPC call failed");

	assert!(hash.is_some(), "Should return hash when no block number provided");
	assert_eq!(hash.unwrap(), format!("0x{}", hex::encode(expected_hash.as_bytes())));
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_block_hash_returns_fork_point_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let fork_point_number = ctx.blockchain().fork_point_number();
	let expected_hash = ctx.blockchain().fork_point();

	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();

	let hash: Option<String> = client
		.request("chain_getBlockHash", rpc_params![fork_point_number])
		.await
		.expect("RPC call failed");

	assert!(hash.is_some(), "Should return hash for fork point");
	assert_eq!(hash.unwrap(), format!("0x{}", hex::encode(expected_hash.as_bytes())));
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_block_hash_returns_historical_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let fork_point_number = ctx.blockchain().fork_point_number();

	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();
	ctx.blockchain().build_empty_block().await.unwrap();

	// Only test if fork point is > 0 (has blocks before it)
	if fork_point_number > 0 {
		let historical_number = fork_point_number - 1;

		let hash: Option<String> = client
			.request("chain_getBlockHash", rpc_params![historical_number])
			.await
			.expect("RPC call failed");

		assert!(hash.is_some(), "Should return hash for historical block");
		let hash = hash.unwrap();
		assert!(hash.starts_with("0x"), "Hash should start with 0x");
		assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_header_returns_valid_header() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Build a block
	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let hash = format!("0x{}", hex::encode(block.hash.as_bytes()));

	let header: Option<RpcHeader> = client
		.request("chain_getHeader", rpc_params![hash])
		.await
		.expect("RPC call failed");

	assert!(header.is_some(), "Should return header");
	let header = header.unwrap();

	// Verify header fields are properly formatted as hex strings
	assert_eq!(header.parent_hash, format!("0x{}", hex::encode(block.parent_hash.as_bytes())));
	assert_eq!(header.number, format!("0x{:x}", block.number));
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_header_returns_head_when_no_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Build a block
	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");

	// Query without hash (should return head)
	let header: Option<RpcHeader> =
		client.request("chain_getHeader", rpc_params![]).await.expect("RPC call failed");

	assert!(header.is_some(), "Should return header when no hash provided");
	assert_eq!(header.unwrap().number, format!("0x{:x}", block.number));
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_header_for_fork_point() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let fork_point_hash = ctx.blockchain().fork_point();
	let hash = format!("0x{}", hex::encode(fork_point_hash.as_bytes()));

	let header: Option<RpcHeader> = client
		.request("chain_getHeader", rpc_params![hash])
		.await
		.expect("RPC call failed");

	assert!(header.is_some(), "Should return header for fork point");
	assert_eq!(header.unwrap().number, format!("0x{:x}", ctx.blockchain().fork_point_number()));
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_block_returns_full_block() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Build a block
	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let hash = format!("0x{}", hex::encode(block.hash.as_bytes()));

	let signed_block: Option<SignedBlock> = client
		.request("chain_getBlock", rpc_params![hash])
		.await
		.expect("RPC call failed");

	assert!(signed_block.is_some(), "Should return full block");
	let signed_block = signed_block.unwrap();

	// Verify block structure (header fields are hex strings in RPC format)
	assert_eq!(
		signed_block.block.header.parent_hash,
		format!("0x{}", hex::encode(block.parent_hash.as_bytes()))
	);
	assert_eq!(signed_block.block.header.number, format!("0x{:x}", block.number));

	// Extrinsics should be present (at least inherents)
	assert_eq!(
		signed_block.block.extrinsics,
		block
			.extrinsics
			.iter()
			.map(|ext_bytes| format!("0x{}", hex::encode(ext_bytes)))
			.collect::<Vec<_>>()
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_block_returns_head_when_no_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	// Build a block
	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");

	// Query without hash
	let signed_block: Option<SignedBlock> =
		client.request("chain_getBlock", rpc_params![]).await.expect("RPC call failed");

	let signed_block = signed_block.unwrap();
	assert_eq!(signed_block.block.header.number, format!("0x{:x}", block.number));
	assert_eq!(
		signed_block.block.extrinsics,
		block
			.extrinsics
			.iter()
			.map(|ext_bytes| format!("0x{}", hex::encode(ext_bytes)))
			.collect::<Vec<_>>()
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_finalized_head_returns_head_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let expected_hash = ctx.blockchain().head_hash().await;

	let hash: String = client
		.request("chain_getFinalizedHead", rpc_params![])
		.await
		.expect("RPC call failed");

	assert!(hash.starts_with("0x"), "Hash should start with 0x");
	assert_eq!(hash, format!("0x{}", hex::encode(expected_hash.as_bytes())));
}

#[tokio::test(flavor = "multi_thread")]
async fn chain_get_finalized_head_updates_after_block() {
	let ctx = TestContext::for_rpc_server().await;
	let client = WsClientBuilder::default()
		.build(&ctx.ws_url())
		.await
		.expect("Failed to connect");

	let hash_before: String = client
		.request("chain_getFinalizedHead", rpc_params![])
		.await
		.expect("RPC call failed");

	// Build a new block
	let new_block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");

	let hash_after: String = client
		.request("chain_getFinalizedHead", rpc_params![])
		.await
		.expect("RPC call failed");

	// Hash should have changed
	assert_ne!(hash_before, hash_after, "Finalized head should update after new block");
	assert_eq!(hash_after, format!("0x{}", hex::encode(new_block.hash.as_bytes())));
}
