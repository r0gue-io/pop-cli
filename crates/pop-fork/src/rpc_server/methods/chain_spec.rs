// SPDX-License-Identifier: GPL-3.0

//! New chainSpec_v1_* RPC methods.
//!
//! These methods return immutable chain specification data that never changes
//! during the fork's lifetime. Values are fetched lazily on first call and
//! cached per-blockchain instance.

use crate::{Blockchain, rpc_server::RpcServerError};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::sync::Arc;

/// chainSpec RPC methods (v1 spec).
///
/// All values returned by these methods are immutable and cached after first fetch.
#[rpc(server, namespace = "chainSpec")]
pub trait ChainSpecApi {
	/// Get the human-readable chain name.
	///
	/// Returns a string like "Polkadot", "Kusama", "Asset Hub", etc.
	#[method(name = "v1_chainName")]
	async fn chain_name(&self) -> RpcResult<String>;

	/// Get the genesis block hash.
	///
	/// Returns the hex-encoded hash of block 0, prefixed with "0x".
	#[method(name = "v1_genesisHash")]
	async fn genesis_hash(&self) -> RpcResult<String>;

	/// Get the chain properties.
	///
	/// Returns a JSON object containing chain-specific properties like
	/// tokenDecimals, tokenSymbol, ss58Format, etc.
	///
	/// May return `null` if no properties are available.
	#[method(name = "v1_properties")]
	async fn properties(&self) -> RpcResult<Option<serde_json::Value>>;
}

/// Implementation of chainSpec RPC methods.
pub struct ChainSpecApi {
	blockchain: Arc<Blockchain>,
}

impl ChainSpecApi {
	/// Create a new ChainSpecApi instance.
	pub fn new(blockchain: Arc<Blockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl ChainSpecApiServer for ChainSpecApi {
	async fn chain_name(&self) -> RpcResult<String> {
		Ok(self.blockchain.chain_name().to_string())
	}

	async fn genesis_hash(&self) -> RpcResult<String> {
		self.blockchain.genesis_hash().await.map_err(|e| {
			RpcServerError::Internal(format!("Failed to fetch genesis hash: {e}")).into()
		})
	}

	async fn properties(&self) -> RpcResult<Option<serde_json::Value>> {
		Ok(self.blockchain.chain_properties().await)
	}
}
