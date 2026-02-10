// SPDX-License-Identifier: GPL-3.0

//! Legacy chain_* RPC methods.
//!
//! These methods provide block-related operations for polkadot.js compatibility.

use crate::{
	Blockchain, BlockchainEvent,
	rpc_server::{
		RpcServerError, parse_block_hash,
		types::{BlockData, Header, HexString, RpcHeader, SignedBlock},
	},
};
use jsonrpsee::{
	PendingSubscriptionSink,
	core::{RpcResult, SubscriptionResult},
	proc_macros::rpc,
};
use log::{debug, warn};
use scale::Decode;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

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
	async fn get_header(&self, hash: Option<String>) -> RpcResult<Option<RpcHeader>>;

	/// Get full block by hash.
	///
	/// Returns the full signed block with the given hash, or the best block if no hash is provided.
	#[method(name = "getBlock")]
	async fn get_block(&self, hash: Option<String>) -> RpcResult<Option<SignedBlock>>;

	/// Get the hash of the last finalized block.
	#[method(name = "getFinalizedHead")]
	async fn get_finalized_head(&self) -> RpcResult<String>;

	/// Subscribe to new block headers.
	#[subscription(name = "subscribeNewHeads" => "newHead", unsubscribe = "unsubscribeNewHeads", item = RpcHeader)]
	async fn subscribe_new_heads(&self) -> SubscriptionResult;

	/// Subscribe to finalized block headers.
	#[subscription(name = "subscribeFinalizedHeads" => "finalizedHead", unsubscribe = "unsubscribeFinalizedHeads", item = RpcHeader)]
	async fn subscribe_finalized_heads(&self) -> SubscriptionResult;

	/// Subscribe to all block headers (alias for subscribeNewHeads).
	#[subscription(name = "subscribeAllHeads" => "allHead", unsubscribe = "unsubscribeAllHeads", item = RpcHeader)]
	async fn subscribe_all_heads(&self) -> SubscriptionResult;
}

/// Implementation of legacy chain RPC methods.
pub struct ChainApi {
	blockchain: Arc<Blockchain>,
	shutdown_token: CancellationToken,
}

impl ChainApi {
	/// Create a new ChainApi instance.
	pub fn new(blockchain: Arc<Blockchain>, shutdown_token: CancellationToken) -> Self {
		Self { blockchain, shutdown_token }
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
			Ok(Some(hash)) => Ok(Some(HexString::from_bytes(hash.as_bytes()).into())),
			Ok(None) => Ok(None),
			Err(e) =>
				Err(RpcServerError::Internal(format!("Failed to fetch block hash: {e}")).into()),
		}
	}

	async fn get_header(&self, hash: Option<String>) -> RpcResult<Option<RpcHeader>> {
		let block_hash = match hash {
			Some(ref h) => parse_block_hash(h)?,
			None => self.blockchain.head_hash().await,
		};

		debug!("[chain] getHeader requested for {}", hash.as_deref().unwrap_or("(head)"));

		match self.blockchain.block_header(block_hash).await {
			Ok(Some(header_bytes)) => {
				let header = Header::decode(&mut header_bytes.as_slice()).map_err(|e| {
					RpcServerError::Internal(format!("Failed to decode header: {e}"))
				})?;
				debug!(
					"[chain] getHeader returning #{} (parent=0x{}...)",
					header.number,
					hex::encode(&header.parent_hash.0[..4])
				);
				Ok(Some(RpcHeader::from_header(&header)))
			},
			Ok(None) => {
				warn!(
					"[chain] getHeader: block not found for 0x{}",
					hex::encode(&block_hash.0[..8])
				);
				Ok(None)
			},
			Err(e) =>
				Err(RpcServerError::Internal(format!("Failed to fetch block header: {e}")).into()),
		}
	}

	async fn get_block(&self, hash: Option<String>) -> RpcResult<Option<SignedBlock>> {
		let block_hash = match &hash {
			Some(h) => parse_block_hash(h)?,
			None => self.blockchain.head_hash().await,
		};

		debug!("[chain] getBlock requested for {}", hash.as_deref().unwrap_or("(head)"));

		// Get header
		let header = match self
			.get_header(Some(HexString::from_bytes(block_hash.as_bytes()).into()))
			.await?
		{
			Some(h) => h,
			None => {
				warn!(
					"[chain] getBlock: header not found for 0x{}",
					hex::encode(&block_hash.0[..8])
				);
				return Ok(None);
			},
		};

		// Get extrinsics
		let extrinsics = match self.blockchain.block_body(block_hash).await {
			Ok(Some(body)) => body.iter().map(|ext| HexString::from_bytes(ext).into()).collect(),
			Ok(None) => {
				warn!("[chain] getBlock: body not found for 0x{}", hex::encode(&block_hash.0[..8]));
				return Ok(None);
			},
			Err(e) =>
				return Err(
					RpcServerError::Internal(format!("Failed to fetch block body: {e}")).into()
				),
		};

		Ok(Some(SignedBlock { block: BlockData { header, extrinsics }, justifications: None }))
	}

	async fn get_finalized_head(&self) -> RpcResult<String> {
		let hash = self.blockchain.head_hash().await;
		Ok(HexString::from_bytes(hash.as_bytes()).into())
	}

	async fn subscribe_new_heads(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
		let sink = pending.accept().await?;
		let blockchain = Arc::clone(&self.blockchain);
		let token = self.shutdown_token.clone();

		debug!("[chain] New heads subscription accepted");

		// Send current head immediately
		let head_hash = blockchain.head_hash().await;
		match blockchain.block_header(head_hash).await {
			Ok(Some(header_bytes)) => match Header::decode(&mut header_bytes.as_slice()) {
				Ok(header) => {
					// Log the hash that PJS will compute from re-encoding this header
					use scale::Encode;
					use sp_core::hashing::blake2_256;
					let reencoded = header.encode();
					let computed_hash = blake2_256(&reencoded);
					debug!(
						"[chain] Initial head #{}: stored_hash=0x{} computed_hash=0x{} parent=0x{} (header {} bytes, reencoded {} bytes)",
						header.number,
						hex::encode(head_hash.as_bytes()),
						hex::encode(computed_hash),
						hex::encode(header.parent_hash.as_bytes()),
						header_bytes.len(),
						reencoded.len(),
					);
					let rpc_header = RpcHeader::from_header(&header);
					let _ =
						sink.send(jsonrpsee::SubscriptionMessage::from_json(&rpc_header)?).await;
					debug!("[chain] Sent initial head #{}", header.number);
				},
				Err(e) => warn!(
					"[chain] Failed to decode initial head header ({} bytes): {e}",
					header_bytes.len()
				),
			},
			Ok(None) =>
				warn!("[chain] No header found for head hash 0x{}", hex::encode(&head_hash.0[..4])),
			Err(e) => warn!("[chain] Failed to fetch initial head header: {e}"),
		}

		// Subscribe to blockchain events
		let mut receiver = blockchain.subscribe_events();

		// Spawn task to forward events to sink
		tokio::spawn(async move {
			loop {
				tokio::select! {
					biased;

					// Server shutting down
					_ = token.cancelled() => break,

					// Client disconnected
					_ = sink.closed() => {
						debug!("[chain] Subscriber disconnected");
						break;
					},

					// New event received
					event = receiver.recv() => {
						match event {
							Ok(BlockchainEvent::NewBlock { number, header, .. }) => {
								match Header::decode(&mut header.as_slice()) {
									Ok(decoded) => {
										let rpc_header = RpcHeader::from_header(&decoded);
										let msg = match jsonrpsee::SubscriptionMessage::from_json(&rpc_header) {
											Ok(m) => m,
											Err(e) => {
												warn!("[chain] Failed to serialize header for #{number}: {e}");
												continue;
											},
										};
										if sink.send(msg).await.is_err() {
											debug!("[chain] Subscriber disconnected during send");
											break;
										}
										debug!("[chain] Sent new head #{number}");
									},
									Err(e) => warn!(
										"[chain] Failed to decode header for #{number} ({} bytes): {e}",
										header.len()
									),
								}
							}
							Err(broadcast::error::RecvError::Lagged(n)) => {
								warn!("[chain] Subscriber lagged, skipped {n} events");
								continue;
							}
							Err(broadcast::error::RecvError::Closed) => {
								debug!("[chain] Broadcast channel closed");
								break;
							}
						}
					}
				}
			}
		});

		Ok(())
	}

	async fn subscribe_finalized_heads(
		&self,
		pending: PendingSubscriptionSink,
	) -> SubscriptionResult {
		// For a fork, finalized = head
		self.subscribe_new_heads(pending).await
	}

	async fn subscribe_all_heads(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
		self.subscribe_new_heads(pending).await
	}
}

#[cfg(test)]
mod tests {
	use crate::{
		rpc_server::types::{RpcHeader, SignedBlock},
		testing::TestContext,
	};
	use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

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
}
