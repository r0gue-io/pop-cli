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

#[async_trait::async_trait]
pub trait ChainBlockchain: Send + Sync {
	async fn head_number(&self) -> u32;
	async fn head_hash(&self) -> subxt::utils::H256;
	async fn block_hash_at(
		&self,
		number: u32,
	) -> Result<Option<subxt::utils::H256>, crate::BlockchainError>;
	async fn block_header(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<Vec<u8>>, crate::BlockchainError>;
	async fn block_body(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<Vec<Vec<u8>>>, crate::BlockchainError>;
	fn subscribe_events(&self) -> broadcast::Receiver<BlockchainEvent>;
}

#[async_trait::async_trait]
impl ChainBlockchain for Blockchain {
	async fn head_number(&self) -> u32 {
		Blockchain::head_number(self).await
	}

	async fn head_hash(&self) -> subxt::utils::H256 {
		Blockchain::head_hash(self).await
	}

	async fn block_hash_at(
		&self,
		number: u32,
	) -> Result<Option<subxt::utils::H256>, crate::BlockchainError> {
		Blockchain::block_hash_at(self, number).await
	}

	async fn block_header(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
		Blockchain::block_header(self, hash).await
	}

	async fn block_body(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<Vec<Vec<u8>>>, crate::BlockchainError> {
		Blockchain::block_body(self, hash).await
	}

	fn subscribe_events(&self) -> broadcast::Receiver<BlockchainEvent> {
		Blockchain::subscribe_events(self)
	}
}

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
pub struct ChainApi<T: ChainBlockchain = Blockchain> {
	blockchain: Arc<T>,
	shutdown_token: CancellationToken,
}

impl<T: ChainBlockchain> ChainApi<T> {
	/// Create a new ChainApi instance.
	pub fn new(blockchain: Arc<T>, shutdown_token: CancellationToken) -> Self {
		Self { blockchain, shutdown_token }
	}
}

#[async_trait::async_trait]
impl<T: ChainBlockchain + 'static> ChainApiServer for ChainApi<T> {
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
			Err(e) => {
				return Err(
					RpcServerError::Internal(format!("Failed to fetch block body: {e}")).into()
				);
			},
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
			Ok(None) => {
				warn!("[chain] No header found for head hash 0x{}", hex::encode(&head_hash.0[..4]))
			},
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
	use super::*;
	use crate::rpc_server::test_scenarios::chain as scenario;
	use jsonrpsee::server::ServerBuilder;
	use scale::Encode;
	use subxt::config::substrate::{Digest, DynamicHasher256, H256, SubstrateHeader};

	fn mock_hash(byte: u8) -> H256 {
		H256::from([byte; 32])
	}

	fn encode_header(number: u32, parent: H256) -> Vec<u8> {
		let header = SubstrateHeader::<u32, DynamicHasher256> {
			parent_hash: parent,
			number,
			state_root: mock_hash(7),
			extrinsics_root: mock_hash(8),
			digest: Digest::default(),
		};
		header.encode()
	}

	struct MockChainBlockchain {
		head_number: u32,
		head_hash: H256,
		header_by_hash: std::collections::HashMap<H256, Vec<u8>>,
		body_by_hash: std::collections::HashMap<H256, Vec<Vec<u8>>>,
		hash_by_number: std::collections::HashMap<u32, H256>,
	}

	#[async_trait::async_trait]
	impl ChainBlockchain for MockChainBlockchain {
		async fn head_number(&self) -> u32 {
			self.head_number
		}

		async fn head_hash(&self) -> H256 {
			self.head_hash
		}

		async fn block_hash_at(&self, number: u32) -> Result<Option<H256>, crate::BlockchainError> {
			Ok(self.hash_by_number.get(&number).copied())
		}

		async fn block_header(
			&self,
			hash: H256,
		) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
			Ok(self.header_by_hash.get(&hash).cloned())
		}

		async fn block_body(
			&self,
			hash: H256,
		) -> Result<Option<Vec<Vec<u8>>>, crate::BlockchainError> {
			Ok(self.body_by_hash.get(&hash).cloned())
		}

		fn subscribe_events(&self) -> broadcast::Receiver<BlockchainEvent> {
			let (_, rx) = broadcast::channel(1);
			rx
		}
	}

	fn mock_api() -> ChainApi<MockChainBlockchain> {
		let head_hash = mock_hash(1);
		let parent_hash = mock_hash(2);
		let mut header_by_hash = std::collections::HashMap::new();
		header_by_hash.insert(head_hash, encode_header(10, parent_hash));

		let mut body_by_hash = std::collections::HashMap::new();
		body_by_hash.insert(head_hash, vec![vec![0xaa, 0xbb], vec![0xcc]]);

		let mut hash_by_number = std::collections::HashMap::new();
		hash_by_number.insert(10, head_hash);

		let blockchain = MockChainBlockchain {
			head_number: 10,
			head_hash,
			header_by_hash,
			body_by_hash,
			hash_by_number,
		};
		ChainApi::new(Arc::new(blockchain), CancellationToken::new())
	}

	async fn start_mock_server() -> (String, jsonrpsee::server::ServerHandle) {
		let server =
			ServerBuilder::default().build("127.0.0.1:0").await.expect("server should bind");
		let addr = server.local_addr().expect("server should expose local addr");
		let handle = server.start(ChainApiServer::into_rpc(mock_api()));
		(format!("ws://{}", addr), handle)
	}

	#[tokio::test]
	async fn chain_get_block_hash_returns_head_hash() {
		let (ws_url, handle) = start_mock_server().await;
		scenario::chain_get_block_hash_returns_head_hash(
			&ws_url,
			10,
			&format!("0x{}", "01".repeat(32)),
		)
		.await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn chain_get_block_hash_returns_none_hash() {
		let (ws_url, handle) = start_mock_server().await;
		scenario::chain_get_block_hash_returns_none_hash(&ws_url, 999).await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn chain_get_block_hash_without_number_returns_head_hash() {
		let (ws_url, handle) = start_mock_server().await;
		scenario::chain_get_block_hash_without_number_returns_head_hash(
			&ws_url,
			&format!("0x{}", "01".repeat(32)),
		)
		.await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn chain_get_header_returns_valid_header() {
		let (ws_url, handle) = start_mock_server().await;
		scenario::chain_get_header_returns_valid_header(
			&ws_url,
			&format!("0x{}", "01".repeat(32)),
			"0xa",
			&format!("0x{}", "02".repeat(32)),
		)
		.await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn chain_get_header_returns_head_when_no_hash() {
		let (ws_url, handle) = start_mock_server().await;
		scenario::chain_get_header_returns_head_when_no_hash(&ws_url, "0xa").await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn chain_get_block_returns_full_block() {
		let (ws_url, handle) = start_mock_server().await;
		scenario::chain_get_block_returns_full_block(
			&ws_url,
			&format!("0x{}", "01".repeat(32)),
			"0xa",
			&format!("0x{}", "02".repeat(32)),
			&["0xaabb".to_string(), "0xcc".to_string()],
		)
		.await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn chain_get_block_returns_head_when_no_hash() {
		let (ws_url, handle) = start_mock_server().await;
		scenario::chain_get_block_returns_head_when_no_hash(&ws_url, "0xa").await;
		handle.stop().expect("server should stop");
	}

	#[tokio::test]
	async fn chain_get_finalized_head_returns_head_hash() {
		let (ws_url, handle) = start_mock_server().await;
		scenario::chain_get_finalized_head_returns_head_hash(
			&ws_url,
			&format!("0x{}", "01".repeat(32)),
		)
		.await;
		handle.stop().expect("server should stop");
	}
}
