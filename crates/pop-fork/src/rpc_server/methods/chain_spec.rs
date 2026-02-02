// SPDX-License-Identifier: GPL-3.0

//! New chainSpec_v1_* RPC methods.
//!
//! These methods return immutable chain specification data that never changes
//! during the fork's lifetime. Values are fetched lazily on first call and
//! cached in static memory.

use crate::{
	Blockchain,
	rpc::ForkRpcClient,
	rpc_server::{RpcServerError, types::HexString},
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::sync::{Arc, OnceLock};

/// Static cache for genesis hash (shared with archive.rs).
pub static GENESIS_HASH: OnceLock<String> = OnceLock::new();

/// Static cache for chain properties.
pub static CHAIN_PROPERTIES: OnceLock<Option<serde_json::Value>> = OnceLock::new();

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
		// Return cached value if available
		if let Some(hash) = GENESIS_HASH.get() {
			return Ok(hash.clone());
		}

		// Fetch genesis hash (block 0)
		match self.blockchain.block_hash_at(0).await {
			Ok(Some(hash)) => {
				let formatted: String = HexString::from_bytes(hash.as_bytes()).into();
				Ok(GENESIS_HASH.get_or_init(|| formatted).clone())
			},
			Ok(None) =>
				Err(RpcServerError::BlockNotFound("Genesis block not found".to_string()).into()),
			Err(e) =>
				Err(RpcServerError::Internal(format!("Failed to fetch genesis hash: {e}")).into()),
		}
	}

	async fn properties(&self) -> RpcResult<Option<serde_json::Value>> {
		// Return cached value if available
		if let Some(props) = CHAIN_PROPERTIES.get() {
			return Ok(props.clone());
		}

		// Fetch chain properties from upstream
		let props = match ForkRpcClient::connect(self.blockchain.endpoint()).await {
			Ok(client) => match client.system_properties().await {
				Ok(system_props) => serde_json::to_value(system_props).ok(),
				Err(_) => None,
			},
			Err(_) => None,
		};

		Ok(CHAIN_PROPERTIES.get_or_init(|| props).clone())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		TxPool,
		rpc_server::{ForkRpcServer, RpcServerConfig},
	};
	use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};
	use pop_common::test_env::TestNode;
	use url::Url;

	/// Test context holding spawned node and RPC server.
	struct RpcTestContext {
		#[allow(dead_code)]
		node: TestNode,
		#[allow(dead_code)]
		server: ForkRpcServer,
		ws_url: String,
	}

	/// Creates a test context with spawned node and RPC server.
	async fn setup_rpc_test() -> RpcTestContext {
		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");

		let blockchain =
			Blockchain::fork(&endpoint, None).await.expect("Failed to fork blockchain");
		let txpool = Arc::new(TxPool::new());

		let server = ForkRpcServer::start(blockchain.clone(), txpool, RpcServerConfig::default())
			.await
			.expect("Failed to start RPC server");

		let ws_url = server.ws_url();
		RpcTestContext { node, server, ws_url }
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_spec_chain_name_returns_string() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let name: String = client
			.request("chainSpec_v1_chainName", rpc_params![])
			.await
			.expect("RPC call failed");

		// Chain name should not be empty
		assert!(!name.is_empty(), "Chain name should not be empty");

		// Should match blockchain's chain_name
		assert_eq!(name, "ink-node");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_spec_genesis_hash_returns_valid_hex_hash() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let hash: String = client
			.request("chainSpec_v1_genesisHash", rpc_params![])
			.await
			.expect("RPC call failed");

		// Hash should be properly formatted
		assert!(hash.starts_with("0x"), "Hash should start with 0x");
		assert_eq!(hash.len(), 66, "Hash should be 0x + 64 hex chars");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_spec_genesis_hash_matches_archive() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Get genesis hash via chainSpec
		let chain_spec_hash: String = client
			.request("chainSpec_v1_genesisHash", rpc_params![])
			.await
			.expect("chainSpec RPC call failed");

		// Get genesis hash via archive
		let archive_hash: String = client
			.request("archive_v1_genesisHash", rpc_params![])
			.await
			.expect("archive RPC call failed");

		// Both should return the same value
		assert_eq!(
			chain_spec_hash, archive_hash,
			"chainSpec and archive genesis hashes should match"
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_spec_properties_returns_json_or_null() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let properties: Option<serde_json::Value> = client
			.request("chainSpec_v1_properties", rpc_params![])
			.await
			.expect("RPC call failed");

		// Properties can be Some or None, both are valid
		// If present, should be an object
		if let Some(props) = &properties {
			assert!(props.is_object(), "Properties should be a JSON object");
		}
	}
}
