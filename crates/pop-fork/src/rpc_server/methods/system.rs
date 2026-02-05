// SPDX-License-Identifier: GPL-3.0

//! Legacy system_* RPC methods.
//!
//! These methods provide system information for polkadot.js compatibility.

use super::chain_spec::CHAIN_PROPERTIES;
use crate::{
	Blockchain, ForkRpcClient,
	rpc_server::{
		RpcServerError,
		types::{SyncState, SystemHealth},
	},
	strings::rpc_server::{storage, system},
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sp_core::crypto::{AccountId32, Ss58Codec};
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

	/// Get the local peer ID.
	#[method(name = "localPeerId")]
	fn local_peer_id(&self) -> RpcResult<String>;

	/// Get the node roles.
	#[method(name = "nodeRoles")]
	fn node_roles(&self) -> RpcResult<Vec<String>>;

	/// Get local listen addresses.
	#[method(name = "localListenAddresses")]
	fn local_listen_addresses(&self) -> RpcResult<Vec<String>>;

	/// Get the chain type.
	#[method(name = "chainType")]
	fn chain_type(&self) -> RpcResult<String>;

	/// Get the sync state.
	#[method(name = "syncState")]
	async fn sync_state(&self) -> RpcResult<SyncState>;

	/// Get the next account nonce (index).
	///
	/// Returns the next available nonce for an account, which is used for transaction ordering.
	/// For non-existent accounts, returns 0.
	#[method(name = "accountNextIndex")]
	async fn account_next_index(&self, account: String) -> RpcResult<u32>;
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
		Ok(system::NODE_NAME.to_string())
	}

	async fn version(&self) -> RpcResult<String> {
		// Return the pop-fork version
		Ok(system::NODE_VERSION.to_string())
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

	fn local_peer_id(&self) -> RpcResult<String> {
		// Return a mock peer ID for the fork
		Ok(system::MOCK_PEER_ID.to_string())
	}

	fn node_roles(&self) -> RpcResult<Vec<String>> {
		// Fork acts as a full node
		Ok(vec![system::NODE_ROLE_FULL.to_string()])
	}

	fn local_listen_addresses(&self) -> RpcResult<Vec<String>> {
		// Fork doesn't listen on p2p addresses
		Ok(vec![])
	}

	fn chain_type(&self) -> RpcResult<String> {
		// Fork is always a development chain
		Ok(system::CHAIN_TYPE_DEVELOPMENT.to_string())
	}

	async fn sync_state(&self) -> RpcResult<SyncState> {
		// Fork is always fully synced to its current head
		let head = self.blockchain.head_number().await;
		Ok(SyncState { starting_block: 0, current_block: head, highest_block: head })
	}

	async fn account_next_index(&self, account: String) -> RpcResult<u32> {
		// Parse SS58 address to get account bytes
		let account_id = AccountId32::from_ss58check(&account).map_err(|_| {
			RpcServerError::InvalidParam(format!("Invalid SS58 address: {}", account))
		})?;
		let account_bytes: [u8; 32] = account_id.into();

		// Build storage key for System::Account
		let mut key = Vec::new();
		key.extend(sp_core::twox_128(storage::SYSTEM_PALLET));
		key.extend(sp_core::twox_128(storage::ACCOUNT_STORAGE));
		key.extend(sp_core::blake2_128(&account_bytes));
		key.extend(&account_bytes);

		// Query storage at current head
		let block_number = self.blockchain.head_number().await;
		match self.blockchain.storage_at(block_number, &key).await {
			Ok(Some(data)) if data.len() >= storage::NONCE_SIZE => {
				// Nonce is the first u32 in AccountInfo (SCALE encoded as little-endian)
				let nonce = u32::from_le_bytes(
					data[..storage::NONCE_SIZE].try_into().unwrap_or([0; storage::NONCE_SIZE]),
				);
				Ok(nonce)
			},
			_ => Ok(0), // Account doesn't exist, nonce is 0
		}
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

	/// Well-known dev account: Alice
	const ALICE_SS58: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

	#[tokio::test(flavor = "multi_thread")]
	async fn account_next_index_returns_nonce() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Query Alice's nonce (should be 0 or some positive value on dev chain)
		let nonce: u32 = client
			.request("system_accountNextIndex", rpc_params![ALICE_SS58])
			.await
			.expect("RPC call failed");

		// Nonce should be a valid u32 (including 0)
		assert!(nonce < u32::MAX, "Nonce should be a valid value");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn account_next_index_returns_zero_for_nonexistent() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Query a nonexistent account (random valid SS58 address)
		// This is a valid SS58 address but unlikely to have any balance
		let nonexistent_account = "5C4hrfjw9DjXZTzV3MwzrrAr9P1MJhSrvWGWqi1eSuyUpnhM";

		let nonce: u32 = client
			.request("system_accountNextIndex", rpc_params![nonexistent_account])
			.await
			.expect("RPC call failed");

		// Nonexistent account should have nonce 0
		assert_eq!(nonce, 0, "Nonexistent account should have nonce 0");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn account_next_index_invalid_address_returns_error() {
		let ctx = setup_rpc_test().await;
		let client =
			WsClientBuilder::default().build(&ctx.ws_url).await.expect("Failed to connect");

		// Try with an invalid SS58 address
		let result: Result<u32, _> = client
			.request("system_accountNextIndex", rpc_params!["not_a_valid_address"])
			.await;

		assert!(result.is_err(), "Invalid SS58 address should return an error");
	}
}
