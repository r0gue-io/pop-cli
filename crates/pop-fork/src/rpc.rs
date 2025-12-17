// SPDX-License-Identifier: GPL-3.0

//! RPC client wrapper for connecting to live Polkadot-SDK chains.
//!
//! Provides fork-specific RPC functionality for lazy-loading storage from live chains.
//!
//! # Design Decision: Why This Wrapper Exists
//!
//! This module wraps subxt's [`LegacyRpcMethods`], which means we are **duplicating**
//! some of subxt's API surface. We could use `LegacyRpcMethods` directly throughout
//! the codebase, but we chose to add this layer for the following reasons:
//!
//! 1. **Focused API surface**: `LegacyRpcMethods` exposes many methods we don't need. This wrapper
//!    exposes only what's relevant for fork operations, making the crate easier to understand and
//!    use.
//!
//! 2. **Ergonomic error handling**: subxt's errors are generic. This wrapper provides
//!    [`RpcClientError`] with fork-specific error variants and messages.
//!
//! 3. **Convenience methods**: Methods like [`ForkRpcClient::runtime_code`] encapsulate domain
//!    knowledge (fetching the `:code` storage key) that would otherwise be scattered across the
//!    codebase.
//!
//! 4. **Insulation from subxt internals**: If subxt changes its API, we only need to update this
//!    wrapper rather than every call site.
//!
//! The tradeoff is maintaining this thin layer, but we believe the ergonomic benefits
//! justify the small amount of extra code.
//!
//! # Why Legacy RPCs?
//!
//! We use subxt's `LegacyRpcMethods` (`state_*`, `chain_*`) rather than the newer
//! `chainHead_v1_*` or `archive_v1_*` specifications because:
//!
//! 1. **Universal support**: Legacy RPCs work with all Polkadot SDK nodes. The newer specs may not
//!    be available on all endpoints.
//!
//! 2. **Simplicity**: Legacy RPCs use request/response patterns. The new specs require subscription
//!    lifecycle management (follow/unfollow, pin/unpin) which adds complexity for our use case of
//!    querying a specific historical block.
//!
//! 3. **Precedent**: Tools like [chopsticks](https://github.com/AcalaNetwork/chopsticks) use legacy
//!    RPCs for fetching from upstream chains.
//!
//! Note: subxt marks legacy methods as "not advised" but they remain widely used.
//! This decision should be revisited if the ecosystem moves away from legacy RPCs.

use crate::{
	error::rpc::RpcClientError,
	strings::rpc::{methods, storage_keys},
};
use subxt::{
	SubstrateConfig,
	backend::{legacy::LegacyRpcMethods, rpc::RpcClient},
	config::substrate::H256,
};
use url::Url;

/// RPC client wrapper for fork operations.
///
/// Wraps subxt's [`LegacyRpcMethods`] to provide a focused API for fetching state
/// from live Polkadot-SDK chains. See the module-level documentation for why this
/// wrapper exists rather than using `LegacyRpcMethods` directly.
///
/// # Example
///
/// ```ignore
/// use pop_fork::ForkRpcClient;
///
/// let client = ForkRpcClient::connect(&"wss://rpc.polkadot.io".parse()?).await?;
/// let block_hash = client.finalized_head().await?;
/// let metadata = client.metadata(block_hash).await?;
/// let storage_value = client.storage(&key, block_hash).await?;
/// ```
#[derive(Clone, Debug)]
pub struct ForkRpcClient {
	legacy: LegacyRpcMethods<SubstrateConfig>,
	endpoint: Url,
}

impl ForkRpcClient {
	/// Connect to a live Polkadot-SDK chain.
	///
	/// # Arguments
	/// * `endpoint` - WebSocket URL of the chain's RPC endpoint (e.g., `wss://rpc.polkadot.io`)
	///
	/// # Example
	/// ```ignore
	/// let client = ForkRpcClient::connect(&"wss://rpc.polkadot.io".parse()?).await?;
	/// ```
	pub async fn connect(endpoint: &Url) -> Result<Self, RpcClientError> {
		let client = RpcClient::from_url(endpoint.as_str()).await.map_err(|e| {
			RpcClientError::ConnectionFailed {
				endpoint: endpoint.to_string(),
				message: e.to_string(),
			}
		})?;

		let legacy = LegacyRpcMethods::new(client);

		Ok(Self { legacy, endpoint: endpoint.clone() })
	}

	/// Get the endpoint URL this client is connected to.
	pub fn endpoint(&self) -> &Url {
		&self.endpoint
	}

	/// Get the latest finalized block hash.
	///
	/// This is typically the starting point for forking - we fork from the latest
	/// finalized state to ensure consistency.
	pub async fn finalized_head(&self) -> Result<H256, RpcClientError> {
		self.legacy
			.chain_get_finalized_head()
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::CHAIN_GET_FINALIZED_HEAD,
				message: e.to_string(),
			})
	}

	/// Get block header by hash.
	///
	/// Returns the header for the specified block, which contains the parent hash,
	/// state root, extrinsics root, and digest.
	pub async fn header(
		&self,
		hash: H256,
	) -> Result<<SubstrateConfig as subxt::Config>::Header, RpcClientError> {
		self.legacy
			.chain_get_header(Some(hash))
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::CHAIN_GET_HEADER,
				message: e.to_string(),
			})?
			.ok_or_else(|| RpcClientError::InvalidResponse(format!("No header found for {hash:?}")))
	}

	/// Get a single storage value at a specific block.
	///
	/// # Arguments
	/// * `key` - The storage key (raw bytes)
	/// * `at` - The block hash to query state at
	///
	/// # Returns
	/// * `Ok(Some(value))` - Storage exists with value
	/// * `Ok(None)` - Storage key doesn't exist (empty)
	/// * `Err(_)` - RPC error
	pub async fn storage(&self, key: &[u8], at: H256) -> Result<Option<Vec<u8>>, RpcClientError> {
		self.legacy.state_get_storage(key, Some(at)).await.map_err(|e| {
			RpcClientError::RequestFailed {
				method: methods::STATE_GET_STORAGE,
				message: e.to_string(),
			}
		})
	}

	/// Get multiple storage values in a single batch request.
	///
	/// More efficient than multiple individual `storage()` calls when fetching
	/// many keys at once.
	///
	/// # Arguments
	/// * `keys` - Slice of storage keys to fetch
	/// * `at` - The block hash to query state at
	///
	/// # Returns
	/// A vector of optional values, in the same order as the input keys.
	pub async fn storage_batch(
		&self,
		keys: &[&[u8]],
		at: H256,
	) -> Result<Vec<Option<Vec<u8>>>, RpcClientError> {
		if keys.is_empty() {
			return Ok(vec![]);
		}

		let result = self
			.legacy
			.state_query_storage_at(keys.iter().copied(), Some(at))
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::STATE_QUERY_STORAGE_AT,
				message: e.to_string(),
			})?;

		// Build a map of key -> value from the response
		let mut changes: std::collections::HashMap<Vec<u8>, Option<Vec<u8>>> = result
			.into_iter()
			.flat_map(|change_set| {
				change_set.changes.into_iter().map(|(k, v)| {
					let key_bytes = k.0.to_vec();
					let value_bytes = v.map(|v| v.0.to_vec());
					(key_bytes, value_bytes)
				})
			})
			.collect();

		// Return values in the same order as input keys.
		// Use remove() to avoid cloning potentially large storage values.
		// Note: If duplicate keys are passed, only the first occurrence gets the value.
		let values = keys.iter().map(|key| changes.remove(*key).flatten()).collect();

		Ok(values)
	}

	/// Get storage keys matching a prefix, with pagination.
	///
	/// Useful for iterating over map storage items.
	///
	/// # Arguments
	/// * `prefix` - The storage key prefix to match
	/// * `count` - Maximum number of keys to return
	/// * `start_key` - Optional key to start from (for pagination)
	/// * `at` - The block hash to query state at
	pub async fn storage_keys_paged(
		&self,
		prefix: &[u8],
		count: u32,
		start_key: Option<&[u8]>,
		at: H256,
	) -> Result<Vec<Vec<u8>>, RpcClientError> {
		self.legacy
			.state_get_keys_paged(prefix, count, start_key, Some(at))
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::STATE_GET_KEYS_PAGED,
				message: e.to_string(),
			})
	}

	/// Get runtime metadata at a specific block.
	///
	/// Returns the raw metadata bytes which can be parsed using `subxt::Metadata`.
	pub async fn metadata(&self, at: H256) -> Result<Vec<u8>, RpcClientError> {
		let metadata = self.legacy.state_get_metadata(Some(at)).await.map_err(|e| {
			RpcClientError::RequestFailed {
				method: methods::STATE_GET_METADATA,
				message: e.to_string(),
			}
		})?;

		Ok(metadata.into_raw())
	}

	/// Get the runtime WASM code at a specific block.
	///
	/// This fetches the `:code` storage key which contains the runtime WASM blob.
	pub async fn runtime_code(&self, at: H256) -> Result<Vec<u8>, RpcClientError> {
		// :code storage key.
		let code_key = sp_core::storage::well_known_keys::CODE;

		self.storage(code_key, at)
			.await?
			.ok_or_else(|| RpcClientError::StorageNotFound(storage_keys::CODE.to_string()))
	}

	/// Get the chain name from system properties.
	pub async fn system_chain(&self) -> Result<String, RpcClientError> {
		self.legacy.system_chain().await.map_err(|e| RpcClientError::RequestFailed {
			method: methods::SYSTEM_CHAIN,
			message: e.to_string(),
		})
	}

	/// Get system properties (token decimals, symbols, etc.).
	pub async fn system_properties(
		&self,
	) -> Result<subxt::backend::legacy::rpc_methods::SystemProperties, RpcClientError> {
		self.legacy
			.system_properties()
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::SYSTEM_PROPERTIES,
				message: e.to_string(),
			})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn error_display_connection_failed() {
		let err = RpcClientError::ConnectionFailed {
			endpoint: "wss://example.com".to_string(),
			message: "connection refused".to_string(),
		};
		assert_eq!(err.to_string(), "Failed to connect to wss://example.com: connection refused");
	}

	#[test]
	fn error_display_request_failed() {
		let err = RpcClientError::RequestFailed {
			method: methods::STATE_GET_STORAGE,
			message: "connection reset".to_string(),
		};
		assert_eq!(
			err.to_string(),
			format!("RPC request `{}` failed: connection reset", methods::STATE_GET_STORAGE)
		);
	}

	#[test]
	fn error_display_timeout() {
		let err = RpcClientError::Timeout { method: methods::STATE_GET_METADATA };
		assert_eq!(
			err.to_string(),
			format!("RPC request `{}` timed out", methods::STATE_GET_METADATA)
		);
	}

	#[test]
	fn error_display_invalid_response() {
		let err = RpcClientError::InvalidResponse("missing field".to_string());
		assert_eq!(err.to_string(), "Invalid RPC response: missing field");
	}

	#[test]
	fn error_display_storage_not_found() {
		let err = RpcClientError::StorageNotFound(storage_keys::CODE.to_string());
		assert_eq!(
			err.to_string(),
			format!("Required storage key not found: {}", storage_keys::CODE)
		);
	}

	#[tokio::test]
	async fn connect_to_invalid_endpoint_fails() {
		// Use a port that's unlikely to have anything listening
		let endpoint: Url = "ws://127.0.0.1:19999".parse().unwrap();
		let result = ForkRpcClient::connect(&endpoint).await;

		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(
			matches!(err, RpcClientError::ConnectionFailed { .. }),
			"Expected ConnectionFailed, got: {err:?}"
		);
	}

	mod sequential {
		use super::*;
		use pop_common::test_env::TestNode;

		/// System pallet prefix: twox128("System")
		const SYSTEM_PALLET_PREFIX: &str = "26aa394eea5630e07c48ae0c9558cef7";

		/// System::Number storage key: twox128("System") ++ twox128("Number")
		const SYSTEM_NUMBER_KEY: &str =
			"26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac";

		/// System::ParentHash storage key: twox128("System") ++ twox128("ParentHash")
		const SYSTEM_PARENT_HASH_KEY: &str =
			"26aa394eea5630e07c48ae0c9558cef734abf5cb34d6244378cddbf18e849d96";

		#[tokio::test]
		async fn connect_to_node() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			assert_eq!(client.endpoint(), &endpoint);
		}

		#[tokio::test]
		async fn fetch_finalized_head() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();
			// Hash should be 32 bytes
			assert_eq!(hash.as_bytes().len(), 32);
		}

		#[tokio::test]
		async fn fetch_header() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();
			let header = client.header(hash).await.unwrap();
			// Header should have a valid state root (32 bytes)
			assert_eq!(header.state_root.as_bytes().len(), 32);
		}

		#[tokio::test]
		async fn fetch_storage() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();

			let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
			let value = client.storage(&key, hash).await.unwrap();

			// System::Number should exist and have a value
			assert!(value.is_some());
		}

		#[tokio::test]
		async fn fetch_metadata() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();
			let metadata = client.metadata(hash).await.unwrap();

			// Metadata should be substantial
			assert!(metadata.len() > 1000);
		}

		#[tokio::test]
		async fn fetch_runtime_code() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();
			let code = client.runtime_code(hash).await.unwrap();

			// Runtime code should be substantial
			// ink-node runtime is smaller than relay chains but still significant
			assert!(
				code.len() > 10_000,
				"Runtime code should be substantial, got {} bytes",
				code.len()
			);
		}

		#[tokio::test]
		async fn fetch_storage_keys_paged() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();

			let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();
			let keys = client.storage_keys_paged(&prefix, 10, None, hash).await.unwrap();

			// Should find some System storage keys
			assert!(!keys.is_empty());
			// All keys should start with the prefix
			for key in &keys {
				assert!(key.starts_with(&prefix));
			}
		}

		#[tokio::test]
		async fn fetch_storage_batch() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();

			let keys = vec![
				hex::decode(SYSTEM_NUMBER_KEY).unwrap(),
				hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap(),
			];
			let values = client.storage_batch(&keys, hash).await.unwrap();

			assert_eq!(values.len(), 2);
			// Both System::Number and System::ParentHash should exist
			assert!(values[0].is_some());
			assert!(values[1].is_some());
		}

		#[tokio::test]
		async fn fetch_system_chain() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();

			let chain_name = client.system_chain().await.unwrap();

			// Chain should return a non-empty name
			assert!(!chain_name.is_empty());
		}

		#[tokio::test]
		async fn fetch_system_properties() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();

			// Just verify the call succeeds - ink-node may not have all standard properties
			let _properties = client.system_properties().await.unwrap();
		}

		#[tokio::test]
		async fn fetch_header_non_existent_block_fails() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();

			// Use a fabricated block hash that doesn't exist
			let non_existent_hash = H256::from([0xde; 32]);
			let result = client.header(non_existent_hash).await;

			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(
				matches!(err, RpcClientError::InvalidResponse(_)),
				"Expected InvalidResponse for non-existent block, got: {err:?}"
			);
		}

		#[tokio::test]
		async fn fetch_storage_non_existent_key_returns_none() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();

			// Use a fabricated storage key that doesn't exist
			let non_existent_key = vec![0xff; 32];
			let result = client.storage(&non_existent_key, hash).await.unwrap();

			// Non-existent storage returns None, not an error
			assert!(result.is_none());
		}

		#[tokio::test]
		async fn fetch_storage_batch_with_mixed_keys() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();

			// Mix of existing and non-existing keys
			let keys = vec![
				hex::decode(SYSTEM_NUMBER_KEY).unwrap(), // exists
				vec![0xff; 32],                          // doesn't exist
			];
			let values = client.storage_batch(&keys, hash).await.unwrap();

			assert_eq!(values.len(), 2);
			assert!(values[0].is_some(), "System::Number should exist");
			assert!(values[1].is_none(), "Fabricated key should not exist");
		}

		#[tokio::test]
		async fn fetch_storage_batch_empty_keys() {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().unwrap();
			let client = ForkRpcClient::connect(&endpoint).await.unwrap();
			let hash = client.finalized_head().await.unwrap();

			// Empty keys should return empty results
			let values = client.storage_batch(&[], hash).await.unwrap();
			assert!(values.is_empty());
		}
	}
}
