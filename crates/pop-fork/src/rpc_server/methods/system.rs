// SPDX-License-Identifier: GPL-3.0

//! Legacy system_* RPC methods.
//!
//! These methods provide system information for polkadot.js compatibility.

use super::chain_spec::CHAIN_PROPERTIES;
use crate::{Blockchain, ForkRpcClient, rpc_server::types::SystemHealth};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use std::sync::Arc;

/// Legacy system RPC methods.
#[rpc(server, namespace = "system")]
pub trait SystemApi {
	/// Get the chain name.
	#[method(name = "chain")]
	async fn chain(&self) -> RpcResult<String>;

	/// Get the node name.
	#[method(name = "name")]
	async fn name(&self) -> RpcResult<String>;

	/// Get the node version.
	#[method(name = "version")]
	async fn version(&self) -> RpcResult<String>;

	/// Get the node health status.
	#[method(name = "health")]
	async fn health(&self) -> RpcResult<SystemHealth>;

	/// Get the chain properties.
	#[method(name = "properties")]
	async fn properties(&self) -> RpcResult<Option<serde_json::Value>>;
}

/// Implementation of legacy system RPC methods.
pub struct SystemApi {
	blockchain: Arc<Blockchain>,
}

impl SystemApi {
	/// Create a new SystemApi instance.
	pub fn new(blockchain: Arc<Blockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl SystemApiServer for SystemApi {
	async fn chain(&self) -> RpcResult<String> {
		Ok(self.blockchain.chain_name().to_string())
	}

	async fn name(&self) -> RpcResult<String> {
		// Return a descriptive name for the fork
		Ok("pop-fork".to_string())
	}

	async fn version(&self) -> RpcResult<String> {
		// Return the pop-fork version
		Ok("1.0.0".to_string())
	}

	async fn health(&self) -> RpcResult<SystemHealth> {
		// Fork is always "healthy" - no syncing, no peers needed
		Ok(SystemHealth::default())
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
	async fn chain_works() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let name: String =
			client.request("system_chain", rpc_params![]).await.expect("RPC call failed");

		// Chain name should not be empty
		assert!(!name.is_empty(), "Chain name should not be empty");

		// Should match blockchain's chain_name
		assert_eq!(name, "ink-node");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn name_works() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let name: String =
			client.request("system_name", rpc_params![]).await.expect("RPC call failed");

		// Chain name should not be empty
		assert!(!name.is_empty(), "Chain name should not be empty");

		assert_eq!(name, "pop-fork");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn version_works() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let version: String =
			client.request("system_version", rpc_params![]).await.expect("RPC call failed");

		assert_eq!(version, "1.0.0");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn health_works() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let health: SystemHealth =
			client.request("system_health", rpc_params![]).await.expect("RPC call failed");

		// Should match blockchain's chain_name
		assert_eq!(health, SystemHealth::default());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn chain_spec_chain_name() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let name: String =
			client.request("system_chain", rpc_params![]).await.expect("RPC call failed");

		// Chain name should not be empty
		assert!(!name.is_empty(), "Chain name should not be empty");

		// Should match blockchain's chain_name
		assert_eq!(name, "ink-node");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn properties_returns_json_or_null() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		let properties: Option<serde_json::Value> = client
			.request("system_properties", rpc_params![])
			.await
			.expect("RPC call failed");

		// Properties can be Some or None, both are valid
		// If present, should be an object
		if let Some(props) = &properties {
			assert!(props.is_object(), "Properties should be a JSON object");
		}
	}
}
