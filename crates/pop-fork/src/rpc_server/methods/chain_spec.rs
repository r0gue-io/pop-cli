// SPDX-License-Identifier: GPL-3.0

//! New chainSpec_v1_* RPC methods.
//!
//! These methods return immutable chain specification data that never changes
//! during the fork's lifetime. Values are fetched lazily on first call and
//! cached per-blockchain instance.

use crate::{Blockchain, rpc_server::RpcServerError};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::sync::Arc;

#[async_trait::async_trait]
pub trait ChainSpecBlockchain: Send + Sync {
	fn chain_name(&self) -> &str;
	async fn genesis_hash(&self) -> Result<String, crate::BlockchainError>;
	async fn chain_properties(&self) -> Option<serde_json::Value>;
}

#[async_trait::async_trait]
impl ChainSpecBlockchain for Blockchain {
	fn chain_name(&self) -> &str {
		Blockchain::chain_name(self)
	}

	async fn genesis_hash(&self) -> Result<String, crate::BlockchainError> {
		Blockchain::genesis_hash(self).await
	}

	async fn chain_properties(&self) -> Option<serde_json::Value> {
		Blockchain::chain_properties(self).await
	}
}

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
pub struct ChainSpecApi<T: ChainSpecBlockchain = Blockchain> {
	blockchain: Arc<T>,
}

impl<T: ChainSpecBlockchain> ChainSpecApi<T> {
	/// Create a new ChainSpecApi instance.
	pub fn new(blockchain: Arc<T>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl<T: ChainSpecBlockchain + 'static> ChainSpecApiServer for ChainSpecApi<T> {
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

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	struct MockChainSpecBlockchain {
		name: &'static str,
		genesis: String,
		properties: Option<serde_json::Value>,
	}

	#[async_trait::async_trait]
	impl ChainSpecBlockchain for MockChainSpecBlockchain {
		fn chain_name(&self) -> &str {
			self.name
		}

		async fn genesis_hash(&self) -> Result<String, crate::BlockchainError> {
			Ok(self.genesis.clone())
		}

		async fn chain_properties(&self) -> Option<serde_json::Value> {
			self.properties.clone()
		}
	}

	#[tokio::test]
	async fn chain_spec_chain_name_returns_string() {
		let api = ChainSpecApi::new(Arc::new(MockChainSpecBlockchain {
			name: "ink-node",
			genesis: "0x11".to_string(),
			properties: None,
		}));
		assert_eq!(ChainSpecApiServer::chain_name(&api).await.unwrap(), "ink-node");
	}

	#[tokio::test]
	async fn chain_spec_genesis_hash_returns_valid_hex_hash() {
		let expected = format!("0x{}", "ab".repeat(32));
		let api = ChainSpecApi::new(Arc::new(MockChainSpecBlockchain {
			name: "ink-node",
			genesis: expected.clone(),
			properties: None,
		}));
		assert_eq!(ChainSpecApiServer::genesis_hash(&api).await.unwrap(), expected);
	}

	#[tokio::test]
	async fn chain_spec_properties_returns_json_or_null() {
		let props = json!({"tokenSymbol":"UNIT","tokenDecimals":12});
		let api = ChainSpecApi::new(Arc::new(MockChainSpecBlockchain {
			name: "ink-node",
			genesis: "0x11".to_string(),
			properties: Some(props.clone()),
		}));
		assert_eq!(ChainSpecApiServer::properties(&api).await.unwrap(), Some(props));
	}
}
