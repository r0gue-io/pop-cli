// SPDX-License-Identifier: GPL-3.0

//! Legacy chain_* RPC methods.
//!
//! These methods provide block-related operations for polkadot.js compatibility.

use crate::{
	Blockchain,
	rpc_server::types::{BlockData, Header, SignedBlock},
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use scale::Decode;
use std::sync::Arc;
use subxt::config::substrate::H256;

/// Legacy chain RPC methods.
#[rpc(server, namespace = "chain")]
pub trait ChainApi {
	/// Get block hash by number.
	///
	/// Returns the block hash at the given height, or the best block hash if no height is provided.
	#[method(name = "getBlockHash")]
	async fn get_block_hash(&self, block_number: Option<u32>) -> RpcResult<Option<String>>;

	/// Get block header by hash.
	///
	/// Returns the header of the block with the given hash, or the best block header if no hash is
	/// provided.
	#[method(name = "getHeader")]
	async fn get_header(&self, hash: Option<String>) -> RpcResult<Option<Header>>;

	/// Get full block by hash.
	///
	/// Returns the full signed block with the given hash, or the best block if no hash is provided.
	#[method(name = "getBlock")]
	async fn get_block(&self, hash: Option<String>) -> RpcResult<Option<SignedBlock>>;

	/// Get the hash of the last finalized block.
	#[method(name = "getFinalizedHead")]
	async fn get_finalized_head(&self) -> RpcResult<String>;
}

/// Implementation of legacy chain RPC methods.
pub struct ChainApi {
	blockchain: Arc<Blockchain>,
}

impl ChainApi {
	/// Create a new ChainApi instance.
	pub fn new(blockchain: Arc<Blockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl ChainApiServer for ChainApi {
	async fn get_block_hash(&self, block_number: Option<u32>) -> RpcResult<Option<String>> {
		let number = match block_number {
			Some(n) => n,
			None => self.blockchain.head_number().await,
		};

		match self.blockchain.block_hash_at(number).await {
			Ok(Some(hash)) => Ok(Some(format!("0x{}", hex::encode(hash.as_bytes())))),
			Ok(None) => Ok(None),
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to fetch block hash: {e}"),
				None::<()>,
			)),
		}
	}

	async fn get_header(&self, hash: Option<String>) -> RpcResult<Option<Header>> {
		let block_hash = match hash {
			Some(h) => {
				let bytes = hex::decode(h.trim_start_matches("0x")).map_err(|e| {
					jsonrpsee::types::ErrorObjectOwned::owned(
						-32602,
						format!("Invalid hex hash: {e}"),
						None::<()>,
					)
				})?;
				H256::from_slice(&bytes)
			},
			None => self.blockchain.head_hash().await,
		};

		match self.blockchain.block_header(block_hash).await {
			Ok(Some(header_bytes)) =>
				Ok(Some(Header::decode(&mut header_bytes.as_slice()).map_err(|e| {
					jsonrpsee::types::ErrorObjectOwned::owned(
						-32603,
						format!("Failed to decode header: {e}"),
						None::<()>,
					)
				})?)),
			Ok(None) => Ok(None),
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to fetch block header: {e}"),
				None::<()>,
			)),
		}
	}

	async fn get_block(&self, hash: Option<String>) -> RpcResult<Option<SignedBlock>> {
		let block_hash = match &hash {
			Some(h) => {
				let bytes = hex::decode(h.trim_start_matches("0x")).map_err(|e| {
					jsonrpsee::types::ErrorObjectOwned::owned(
						-32602,
						format!("Invalid hex hash: {e}"),
						None::<()>,
					)
				})?;
				H256::from_slice(&bytes)
			},
			None => self.blockchain.head_hash().await,
		};

		// Get header
		let header = match self
			.get_header(Some(format!("0x{}", hex::encode(block_hash.as_bytes()))))
			.await?
		{
			Some(h) => h,
			None => return Ok(None),
		};

		// Get extrinsics
		let extrinsics = match self.blockchain.block_body(block_hash).await {
			Ok(Some(body)) => body.iter().map(|ext| format!("0x{}", hex::encode(ext))).collect(),
			Ok(None) => return Ok(None),
			Err(e) =>
				return Err(jsonrpsee::types::ErrorObjectOwned::owned(
					-32603,
					format!("Failed to fetch block body: {e}"),
					None::<()>,
				)),
		};

		Ok(Some(SignedBlock { block: BlockData { header, extrinsics }, justifications: None }))
	}

	async fn get_finalized_head(&self) -> RpcResult<String> {
		let hash = self.blockchain.head_hash().await;
		Ok(format!("0x{}", hex::encode(hash.as_bytes())))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::testing::RpcTestContext;
	use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_get_block_hash_returns_head_hash() {
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a block so we have something beyond fork point
		ctx.blockchain.build_empty_block().await.expect("Failed to build block");

		let head_number = ctx.blockchain.head_number().await;
		let expected_hash = ctx.blockchain.head_hash().await;

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
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let expected_hash = ctx.blockchain.head_hash().await;

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
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let fork_point_number = ctx.blockchain.fork_point_number();
		let expected_hash = ctx.blockchain.fork_point();

		ctx.blockchain.build_empty_block().await.unwrap();
		ctx.blockchain.build_empty_block().await.unwrap();
		ctx.blockchain.build_empty_block().await.unwrap();

		let hash: Option<String> = client
			.request("chain_getBlockHash", rpc_params![fork_point_number])
			.await
			.expect("RPC call failed");

		assert!(hash.is_some(), "Should return hash for fork point");
		assert_eq!(hash.unwrap(), format!("0x{}", hex::encode(expected_hash.as_bytes())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_get_block_hash_returns_historical_hash() {
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let fork_point_number = ctx.blockchain.fork_point_number();

		ctx.blockchain.build_empty_block().await.unwrap();
		ctx.blockchain.build_empty_block().await.unwrap();
		ctx.blockchain.build_empty_block().await.unwrap();

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
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a block
		let block = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let hash = format!("0x{}", hex::encode(block.hash.as_bytes()));

		let header: Option<Header> = client
			.request("chain_getHeader", rpc_params![hash])
			.await
			.expect("RPC call failed");

		assert!(header.is_some(), "Should return header");
		let header = header.unwrap();

		// Verify header fields are properly formatted
		assert_eq!(header.parent_hash, block.parent_hash);
		assert_eq!(header.number, block.number);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_get_header_returns_head_when_no_hash() {
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a block
		let block = ctx.blockchain.build_empty_block().await.expect("Failed to build block");

		// Query without hash (should return head)
		let header: Option<Header> =
			client.request("chain_getHeader", rpc_params![]).await.expect("RPC call failed");

		assert!(header.is_some(), "Should return header when no hash provided");
		assert_eq!(header.unwrap().number, block.number);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_get_header_for_fork_point() {
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let fork_point_hash = ctx.blockchain.fork_point();
		let hash = format!("0x{}", hex::encode(fork_point_hash.as_bytes()));

		let header: Option<Header> = client
			.request("chain_getHeader", rpc_params![hash])
			.await
			.expect("RPC call failed");

		assert!(header.is_some(), "Should return header for fork point");
		assert_eq!(header.unwrap().number, ctx.blockchain.fork_point_number());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_get_block_returns_full_block() {
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a block
		let block = ctx.blockchain.build_empty_block().await.expect("Failed to build block");
		let hash = format!("0x{}", hex::encode(block.hash.as_bytes()));

		let signed_block: Option<SignedBlock> = client
			.request("chain_getBlock", rpc_params![hash])
			.await
			.expect("RPC call failed");

		assert!(signed_block.is_some(), "Should return full block");
		let signed_block = signed_block.unwrap();

		// Verify block structure
		assert_eq!(signed_block.block.header.parent_hash, block.parent_hash);
		assert_eq!(signed_block.block.header.number, block.number);

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
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Build a block
		let block = ctx.blockchain.build_empty_block().await.expect("Failed to build block");

		// Query without hash
		let signed_block: Option<SignedBlock> =
			client.request("chain_getBlock", rpc_params![]).await.expect("RPC call failed");

		let signed_block = signed_block.unwrap();
		assert_eq!(signed_block.block.header.number, block.number);
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
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let expected_hash = ctx.blockchain.head_hash().await;

		let hash: String = client
			.request("chain_getFinalizedHead", rpc_params![])
			.await
			.expect("RPC call failed");

		assert!(hash.starts_with("0x"), "Hash should start with 0x");
		assert_eq!(hash, format!("0x{}", hex::encode(expected_hash.as_bytes())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_get_finalized_head_updates_after_block() {
		let ctx = RpcTestContext::new().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let hash_before: String = client
			.request("chain_getFinalizedHead", rpc_params![])
			.await
			.expect("RPC call failed");

		// Build a new block
		let new_block = ctx.blockchain.build_empty_block().await.expect("Failed to build block");

		let hash_after: String = client
			.request("chain_getFinalizedHead", rpc_params![])
			.await
			.expect("RPC call failed");

		// Hash should have changed
		assert_ne!(hash_before, hash_after, "Finalized head should update after new block");
		assert_eq!(hash_after, format!("0x{}", hex::encode(new_block.hash.as_bytes())));
	}
}
