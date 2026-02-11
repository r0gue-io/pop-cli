// SPDX-License-Identifier: GPL-3.0

//! Legacy system_* RPC methods.
//!
//! These methods provide system information for polkadot.js compatibility.

use crate::{
	Blockchain,
	rpc_server::{
		RpcServerError,
		types::{SyncState, SystemHealth},
	},
	strings::rpc_server::{storage, system},
};
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sp_core::crypto::{AccountId32, Ss58Codec};
use std::sync::Arc;

#[async_trait::async_trait]
pub trait SystemBlockchain: Send + Sync {
	fn chain_name(&self) -> &str;
	async fn chain_properties(&self) -> Option<serde_json::Value>;
	async fn head_number(&self) -> u32;
	async fn storage_at(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError>;
}

#[async_trait::async_trait]
impl SystemBlockchain for Blockchain {
	fn chain_name(&self) -> &str {
		Blockchain::chain_name(self)
	}

	async fn chain_properties(&self) -> Option<serde_json::Value> {
		Blockchain::chain_properties(self).await
	}

	async fn head_number(&self) -> u32 {
		Blockchain::head_number(self).await
	}

	async fn storage_at(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
		Blockchain::storage_at(self, block_number, key).await
	}
}

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
pub struct SystemApi<T: SystemBlockchain = Blockchain> {
	blockchain: Arc<T>,
}

impl<T: SystemBlockchain> SystemApi<T> {
	/// Create a new SystemApi instance.
	pub fn new(blockchain: Arc<T>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl<T: SystemBlockchain + 'static> SystemApiServer for SystemApi<T> {
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
		Ok(self.blockchain.chain_properties().await)
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
	use serde_json::json;

	const ALICE_SS58: &str = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";

	struct MockSystemBlockchain {
		chain_name: &'static str,
		head_number: u32,
		account_nonce: Option<u32>,
		properties: Option<serde_json::Value>,
	}

	#[async_trait::async_trait]
	impl SystemBlockchain for MockSystemBlockchain {
		fn chain_name(&self) -> &str {
			self.chain_name
		}

		async fn chain_properties(&self) -> Option<serde_json::Value> {
			self.properties.clone()
		}

		async fn head_number(&self) -> u32 {
			self.head_number
		}

		async fn storage_at(
			&self,
			_block_number: u32,
			_key: &[u8],
		) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
			Ok(self.account_nonce.map(|nonce| {
				let mut data = vec![0u8; 16];
				data[..4].copy_from_slice(&nonce.to_le_bytes());
				data
			}))
		}
	}

	fn mock_api(
		chain_name: &'static str,
		head_number: u32,
		account_nonce: Option<u32>,
		properties: Option<serde_json::Value>,
	) -> SystemApi<MockSystemBlockchain> {
		SystemApi::new(Arc::new(MockSystemBlockchain {
			chain_name,
			head_number,
			account_nonce,
			properties,
		}))
	}

	#[tokio::test]
	async fn chain_works() {
		let api = mock_api("ink-node", 10, Some(0), None);
		assert_eq!(SystemApiServer::chain(&api).await.unwrap(), "ink-node");
	}

	#[tokio::test]
	async fn name_works() {
		let api = mock_api("ink-node", 10, Some(0), None);
		assert_eq!(SystemApiServer::name(&api).await.unwrap(), "pop-fork");
	}

	#[tokio::test]
	async fn version_works() {
		let api = mock_api("ink-node", 10, Some(0), None);
		assert_eq!(SystemApiServer::version(&api).await.unwrap(), "1.0.0");
	}

	#[tokio::test]
	async fn health_works() {
		let api = mock_api("ink-node", 10, Some(0), None);
		assert_eq!(SystemApiServer::health(&api).await.unwrap(), SystemHealth::default());
	}

	#[tokio::test]
	async fn properties_returns_json_or_null() {
		let api = mock_api("ink-node", 10, Some(0), Some(json!({"ss58Format": 42})));
		let properties = SystemApiServer::properties(&api).await.unwrap();
		assert_eq!(properties, Some(json!({"ss58Format": 42})));
	}

	#[tokio::test]
	async fn account_next_index_returns_nonce() {
		let api = mock_api("ink-node", 10, Some(7), None);
		let nonce =
			SystemApiServer::account_next_index(&api, ALICE_SS58.to_string()).await.unwrap();
		assert_eq!(nonce, 7);
	}

	#[tokio::test]
	async fn account_next_index_returns_zero_for_nonexistent() {
		let api = mock_api("ink-node", 10, None, None);
		let nonce =
			SystemApiServer::account_next_index(&api, ALICE_SS58.to_string()).await.unwrap();
		assert_eq!(nonce, 0);
	}

	#[tokio::test]
	async fn account_next_index_invalid_address_returns_error() {
		let api = mock_api("ink-node", 10, Some(0), None);
		let result =
			SystemApiServer::account_next_index(&api, "not_a_valid_address".to_string()).await;
		assert!(result.is_err());
	}
}
