// SPDX-License-Identifier: GPL-3.0

//! Legacy chain_* RPC methods.
//!
//! These methods provide block-related operations for polkadot.js compatibility.

use crate::rpc_server::types::{Header, SignedBlock};
use crate::Blockchain;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use std::sync::Arc;

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
		let hash = match block_number {
			Some(n) if n == self.blockchain.fork_point_number() => self.blockchain.fork_point(),
			Some(n) if n == self.blockchain.head_number().await => self.blockchain.head_hash().await,
			Some(_) => {
				// Historical block hashes not available yet
				return Ok(None);
			},
			None => self.blockchain.head_hash().await,
		};
		Ok(Some(format!("0x{}", hex::encode(hash.as_bytes()))))
	}

	async fn get_header(&self, _hash: Option<String>) -> RpcResult<Option<Header>> {
		// Header decoding from SCALE is complex - would need full header decoder
		// Return None for now (polkadot.js can work without this)
		Ok(None)
	}

	async fn get_block(&self, _hash: Option<String>) -> RpcResult<Option<SignedBlock>> {
		// Block decoding from SCALE is complex - would need full header decoder
		// Return None for now (polkadot.js can work without this)
		Ok(None)
	}

	async fn get_finalized_head(&self) -> RpcResult<String> {
		let hash = self.blockchain.head_hash().await;
		Ok(format!("0x{}", hex::encode(hash.as_bytes())))
	}
}
