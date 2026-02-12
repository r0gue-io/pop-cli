// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

use crate::{
	rpc_server::types::{RpcHeader, SignedBlock},
	testing::TestContext,
};
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

pub async fn scenario_chain_get_block_hash_returns_head_hash() {
	let ctx = TestContext::for_rpc_server().await;
	ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let head_number = ctx.blockchain().head_number().await;
	let expected_initial_head_hash =
		format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	chain_get_block_hash_returns_head_hash(&ctx.ws_url(), head_number, &expected_initial_head_hash)
		.await;
}

pub async fn scenario_chain_get_block_hash_returns_none_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let head_number = ctx.blockchain().head_number().await;
	chain_get_block_hash_returns_none_hash(&ctx.ws_url(), head_number + 999).await;
}

pub async fn scenario_chain_get_block_hash_without_number_returns_head_hash() {
	let ctx = TestContext::for_rpc_server().await;
	ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let expected_initial_head_hash =
		format!("0x{}", hex::encode(ctx.blockchain().head_hash().await.as_bytes()));
	chain_get_block_hash_without_number_returns_head_hash(
		&ctx.ws_url(),
		&expected_initial_head_hash,
	)
	.await;
}

pub async fn scenario_chain_get_header_returns_valid_header() {
	let ctx = TestContext::for_rpc_server().await;
	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block_hash_hex = format!("0x{}", hex::encode(block.hash.as_bytes()));
	let parent_hash_hex = format!("0x{}", hex::encode(block.parent_hash.as_bytes()));
	let number_hex = format!("0x{:x}", block.number);
	chain_get_header_returns_valid_header(
		&ctx.ws_url(),
		&block_hash_hex,
		&number_hex,
		&parent_hash_hex,
	)
	.await;
}

pub async fn scenario_chain_get_header_returns_number() {
	let ctx = TestContext::for_rpc_server().await;
	chain_get_header_returns_number(
		&ctx.ws_url(),
		&format!("0x{}", hex::encode(ctx.blockchain().fork_point().as_bytes())),
		&format!("0x{:x}", ctx.blockchain().fork_point_number()),
	)
	.await;
}

pub async fn scenario_chain_get_header_returns_head_when_no_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let number_hex = format!("0x{:x}", block.number);
	chain_get_header_returns_head_when_no_hash(&ctx.ws_url(), &number_hex).await;
}

pub async fn scenario_chain_get_block_returns_full_block() {
	let ctx = TestContext::for_rpc_server().await;
	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let block_hash_hex = format!("0x{}", hex::encode(block.hash.as_bytes()));
	let parent_hash_hex = format!("0x{}", hex::encode(block.parent_hash.as_bytes()));
	let number_hex = format!("0x{:x}", block.number);
	let expected_extrinsics = block
		.extrinsics
		.iter()
		.map(|ext| format!("0x{}", hex::encode(ext)))
		.collect::<Vec<_>>();
	chain_get_block_returns_full_block(
		&ctx.ws_url(),
		&block_hash_hex,
		&number_hex,
		&parent_hash_hex,
		&expected_extrinsics,
	)
	.await;
}

pub async fn scenario_chain_get_block_returns_head_when_no_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	let number_hex = format!("0x{:x}", block.number);
	chain_get_block_returns_head_when_no_hash(&ctx.ws_url(), &number_hex).await;
}

pub async fn scenario_chain_get_finalized_head_returns_head_hash() {
	let ctx = TestContext::for_rpc_server().await;
	let new_block = ctx.blockchain().build_empty_block().await.expect("Failed to build block");
	chain_get_finalized_head_returns_head_hash(
		&ctx.ws_url(),
		&format!("0x{}", hex::encode(new_block.hash.as_bytes())),
	)
	.await;
}

pub async fn chain_get_block_hash_returns_head_hash(
	ws_url: &str,
	head_number: u32,
	expected_hash: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let hash: Option<String> = client
		.request("chain_getBlockHash", rpc_params![head_number])
		.await
		.expect("RPC call failed");

	assert_eq!(hash.as_deref(), Some(expected_hash));
}

pub async fn chain_get_block_hash_returns_none_hash(ws_url: &str, missing_number: u32) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let hash: Option<String> = client
		.request("chain_getBlockHash", rpc_params![missing_number])
		.await
		.expect("RPC call failed");

	assert!(hash.is_none(), "Missing block number should return None");
}

pub async fn chain_get_block_hash_without_number_returns_head_hash(
	ws_url: &str,
	expected_hash: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let hash: Option<String> = client
		.request("chain_getBlockHash", rpc_params![])
		.await
		.expect("RPC call failed");

	assert_eq!(hash.as_deref(), Some(expected_hash));
}

pub async fn chain_get_header_returns_valid_header(
	ws_url: &str,
	hash: &str,
	expected_number_hex: &str,
	expected_parent_hash: &str,
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let header: Option<RpcHeader> = client
		.request("chain_getHeader", rpc_params![hash])
		.await
		.expect("RPC call failed");

	let header = header.expect("header should be present");
	assert_eq!(header.number, expected_number_hex);
	assert_eq!(header.parent_hash, expected_parent_hash);
}

pub async fn chain_get_header_returns_number(ws_url: &str, hash: &str, expected_number_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let header: Option<RpcHeader> = client
		.request("chain_getHeader", rpc_params![hash])
		.await
		.expect("RPC call failed");

	assert_eq!(header.expect("header should be present").number, expected_number_hex);
}

pub async fn chain_get_header_returns_head_when_no_hash(ws_url: &str, expected_number_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let header: Option<RpcHeader> =
		client.request("chain_getHeader", rpc_params![]).await.expect("RPC call failed");

	assert_eq!(header.expect("header should be present").number, expected_number_hex);
}

pub async fn chain_get_block_returns_full_block(
	ws_url: &str,
	hash: &str,
	expected_number_hex: &str,
	expected_parent_hash: &str,
	expected_extrinsics: &[String],
) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let block: Option<SignedBlock> = client
		.request("chain_getBlock", rpc_params![hash])
		.await
		.expect("RPC call failed");

	let block = block.expect("signed block should be present");
	assert_eq!(block.block.header.number, expected_number_hex);
	assert_eq!(block.block.header.parent_hash, expected_parent_hash);
	assert_eq!(block.block.extrinsics, expected_extrinsics);
}

pub async fn chain_get_block_returns_head_when_no_hash(ws_url: &str, expected_number_hex: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let block: Option<SignedBlock> =
		client.request("chain_getBlock", rpc_params![]).await.expect("RPC call failed");

	assert_eq!(
		block.expect("signed block should be present").block.header.number,
		expected_number_hex
	);
}

pub async fn chain_get_finalized_head_returns_head_hash(ws_url: &str, expected_hash: &str) {
	let client = WsClientBuilder::default().build(ws_url).await.expect("Failed to connect");

	let hash: String = client
		.request("chain_getFinalizedHead", rpc_params![])
		.await
		.expect("RPC call failed");

	assert_eq!(hash, expected_hash);
}
