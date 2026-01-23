// SPDX-License-Identifier: GPL-3.0

//! Legacy chain_* RPC methods.
//!
//! These methods provide block-related operations for polkadot.js compatibility.

use crate::rpc_server::types::{Header, SignedBlock};
use crate::rpc_server::MockBlockchain;
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
	blockchain: Arc<MockBlockchain>,
}

impl ChainApi {
	/// Create a new ChainApi instance.
	pub fn new(blockchain: Arc<MockBlockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl ChainApiServer for ChainApi {
	async fn get_block_hash(&self, _block_number: Option<u32>) -> RpcResult<Option<String>> {
		// Mock: return the head hash (or None if no specific block)
		let hash = self.blockchain.head_hash().await;
		Ok(Some(format!("0x{}", hex::encode(hash.as_bytes()))))
	}

	async fn get_header(&self, _hash: Option<String>) -> RpcResult<Option<Header>> {
		// Mock: return empty header
		Ok(None)
	}

	async fn get_block(&self, _hash: Option<String>) -> RpcResult<Option<SignedBlock>> {
		// Mock: return None (no blocks available)
		Ok(None)
	}

	async fn get_finalized_head(&self) -> RpcResult<String> {
		let hash = self.blockchain.head_hash().await;
		Ok(format!("0x{}", hex::encode(hash.as_bytes())))
	}
}
