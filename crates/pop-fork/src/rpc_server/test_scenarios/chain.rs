// SPDX-License-Identifier: GPL-3.0

#![allow(missing_docs)]

use crate::rpc_server::types::{RpcHeader, SignedBlock};
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

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
