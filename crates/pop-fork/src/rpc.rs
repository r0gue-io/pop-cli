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
use scale::{Decode, Encode};
use std::sync::Arc;
use subxt::{
	Metadata, SubstrateConfig,
	backend::{
		legacy::{LegacyRpcMethods, rpc_methods::Block},
		rpc::RpcClient,
	},
	config::substrate::H256,
};
use tokio::sync::{RwLock, Semaphore};
use url::Url;

/// Maximum number of concurrent upstream RPC calls for heavy storage methods.
///
/// Limits parallelism for `storage()`, `storage_batch()`, and `storage_keys_paged()` to prevent
/// overwhelming the upstream WebSocket endpoint when many callers (e.g., polkadot.js sending 14
/// concurrent `state_queryStorageAt` requests) hit the RPC server at once.
const MAX_CONCURRENT_UPSTREAM_CALLS: usize = 4;

/// Oldest metadata version supported.
const METADATA_V14: u32 = 14;
/// Most up-to-date metadata version supported.
const METADATA_LATEST: u32 = 15;

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
#[derive(Clone)]
pub struct ForkRpcClient {
	legacy: Arc<RwLock<LegacyRpcMethods<SubstrateConfig>>>,
	endpoint: Url,
	/// Semaphore limiting concurrent upstream calls for heavy storage methods.
	upstream_semaphore: Arc<Semaphore>,
}

impl std::fmt::Debug for ForkRpcClient {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ForkRpcClient").field("endpoint", &self.endpoint).finish()
	}
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
		let legacy = Self::create_connection(endpoint).await?;
		Ok(Self {
			legacy: Arc::new(RwLock::new(legacy)),
			endpoint: endpoint.clone(),
			upstream_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_UPSTREAM_CALLS)),
		})
	}

	/// Create a new connection to the endpoint.
	async fn create_connection(
		endpoint: &Url,
	) -> Result<LegacyRpcMethods<SubstrateConfig>, RpcClientError> {
		let client = RpcClient::from_url(endpoint.as_str()).await.map_err(|e| {
			RpcClientError::ConnectionFailed {
				endpoint: endpoint.to_string(),
				message: e.to_string(),
			}
		})?;
		Ok(LegacyRpcMethods::new(client))
	}

	/// Reconnect to the upstream RPC endpoint.
	///
	/// Creates a fresh WebSocket connection, replacing the existing one. All clones
	/// of this client share the connection, so reconnecting affects all of them.
	pub async fn reconnect(&self) -> Result<(), RpcClientError> {
		let new_legacy = Self::create_connection(&self.endpoint).await?;
		*self.legacy.write().await = new_legacy;
		Ok(())
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
		self.legacy.read().await.chain_get_finalized_head().await.map_err(|e| {
			RpcClientError::RequestFailed {
				method: methods::CHAIN_GET_FINALIZED_HEAD,
				message: e.to_string(),
			}
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
			.read()
			.await
			.chain_get_header(Some(hash))
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::CHAIN_GET_HEADER,
				message: e.to_string(),
			})?
			.ok_or_else(|| RpcClientError::InvalidResponse(format!("No header found for {hash:?}")))
	}

	/// Get a block hash by its number.
	///
	/// # Arguments
	/// * `block_number` - The block number to query
	///
	/// # Returns
	/// * `Ok(Some(hash))` - Block exists with this hash
	/// * `Ok(None)` - Block number doesn't exist yet
	/// * `Err(_)` - RPC error
	pub async fn block_hash_at(&self, block_number: u32) -> Result<Option<H256>, RpcClientError> {
		self.legacy
			.read()
			.await
			.chain_get_block_hash(Some(block_number.into()))
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::CHAIN_GET_BLOCK_HASH,
				message: e.to_string(),
			})
	}

	/// Get full block data by block number.
	///
	/// This method first fetches the block hash for the given block number using
	/// `chain_getBlockHash`, then fetches the full block data using `chain_getBlock`.
	///
	/// # Arguments
	/// * `block_number` - The block number to query
	///
	/// # Returns
	/// * `Ok(Some((hash, block)))` - Block exists with hash and data
	/// * `Ok(None)` - Block number doesn't exist yet
	/// * `Err(_)` - RPC error
	pub async fn block_by_number(
		&self,
		block_number: u32,
	) -> Result<Option<(H256, Block<SubstrateConfig>)>, RpcClientError> {
		// Get block hash from block number
		let block_hash = self.block_hash_at(block_number).await?;

		let block_hash = match block_hash {
			Some(hash) => hash,
			None => return Ok(None),
		};

		// Get full block data
		let block =
			self.legacy.read().await.chain_get_block(Some(block_hash)).await.map_err(|e| {
				RpcClientError::RequestFailed {
					method: methods::CHAIN_GET_BLOCK,
					message: e.to_string(),
				}
			})?;

		Ok(block.map(|block| (block_hash, block.block)))
	}

	/// Get full block data by block hash.
	///
	/// # Arguments
	/// * `block_hash` - The block hash to query
	///
	/// # Returns
	/// * `Ok(Some(block))` - Block exists
	/// * `Ok(None)` - Block hash not found
	/// * `Err(_)` - RPC error
	pub async fn block_by_hash(
		&self,
		block_hash: H256,
	) -> Result<Option<Block<SubstrateConfig>>, RpcClientError> {
		let block =
			self.legacy.read().await.chain_get_block(Some(block_hash)).await.map_err(|e| {
				RpcClientError::RequestFailed {
					method: methods::CHAIN_GET_BLOCK,
					message: e.to_string(),
				}
			})?;

		Ok(block.map(|b| b.block))
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
		self.legacy.read().await.state_get_storage(key, Some(at)).await.map_err(|e| {
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

		let _permit = self.upstream_semaphore.acquire().await.expect("semaphore closed");
		let result = self
			.legacy
			.read()
			.await
			.state_query_storage_at(keys.iter().copied(), Some(at))
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::STATE_QUERY_STORAGE_AT,
				message: e.to_string(),
			})?;

		// Build a map of key -> value from the response
		let changes: std::collections::HashMap<Vec<u8>, Option<Vec<u8>>> = result
			.into_iter()
			.flat_map(|change_set| {
				change_set.changes.into_iter().map(|(k, v)| {
					let key_bytes = k.0.to_vec();
					let value_bytes = v.map(|v| v.0.to_vec());
					(key_bytes, value_bytes)
				})
			})
			.collect();

		// Return values in the same order as input keys, preserving duplicates.
		let values = keys.iter().map(|key| changes.get::<[u8]>(key).cloned().flatten()).collect();

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
		let _permit = self.upstream_semaphore.acquire().await.expect("semaphore closed");
		self.legacy
			.read()
			.await
			.state_get_keys_paged(prefix, count, start_key, Some(at))
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::STATE_GET_KEYS_PAGED,
				message: e.to_string(),
			})
	}

	/// Get runtime metadata at a specific block.
	///
	/// Attempts to fetch and decode metadata via `state_getMetadata`. If decoding
	/// fails (e.g., due to type registry inconsistencies in the chain's metadata),
	/// falls back to requesting specific metadata versions via
	/// `Metadata_metadata_at_version` runtime API (latest down to V14).
	pub async fn metadata(&self, at: H256) -> Result<Metadata, RpcClientError> {
		let raw = self.legacy.read().await.state_get_metadata(Some(at)).await.map_err(|e| {
			RpcClientError::RequestFailed {
				method: methods::STATE_GET_METADATA,
				message: e.to_string(),
			}
		})?;

		let raw_bytes = raw.into_raw();
		match Metadata::decode(&mut raw_bytes.as_slice()) {
			Ok(metadata) => Ok(metadata),
			Err(default_err) => {
				// Try explicit version requests as fallback.
				for version in (METADATA_V14..=METADATA_LATEST).rev() {
					if let Some(bytes) = self.metadata_at_version(version, at).await? &&
						let Ok(metadata) = Metadata::decode(&mut bytes.as_slice())
					{
						return Ok(metadata);
					}
				}
				Err(RpcClientError::MetadataDecodingFailed(default_err.to_string()))
			},
		}
	}

	/// Request metadata at a specific version via the `Metadata_metadata_at_version`
	/// runtime API.
	///
	/// Returns `Ok(Some(bytes))` if the chain supports the requested version,
	/// `Ok(None)` if it does not, or an error if the RPC call itself fails.
	async fn metadata_at_version(
		&self,
		version: u32,
		at: H256,
	) -> Result<Option<Vec<u8>>, RpcClientError> {
		let result = self
			.legacy
			.read()
			.await
			.state_call("Metadata_metadata_at_version", Some(&version.encode()), Some(at))
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::STATE_CALL,
				message: e.to_string(),
			})?;

		// The runtime returns SCALE-encoded `Option<OpaqueMetadata>` where
		// `OpaqueMetadata` is `Vec<u8>`.
		let opaque: Option<Vec<u8>> = Decode::decode(&mut result.as_slice()).map_err(|e| {
			RpcClientError::InvalidResponse(format!(
				"Failed to decode metadata_at_version response: {e}"
			))
		})?;

		Ok(opaque)
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
		self.legacy
			.read()
			.await
			.system_chain()
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::SYSTEM_CHAIN,
				message: e.to_string(),
			})
	}

	/// Execute a runtime API call via `state_call` on the upstream chain.
	///
	/// This is useful for proxying computationally expensive runtime calls (like metadata
	/// generation) to the upstream node, which has a JIT-compiled runtime and handles them
	/// much faster than the local WASM interpreter.
	pub async fn state_call(
		&self,
		function: &str,
		call_parameters: &[u8],
		at: Option<H256>,
	) -> Result<Vec<u8>, RpcClientError> {
		self.legacy
			.read()
			.await
			.state_call(function, Some(call_parameters), at)
			.await
			.map_err(|e| RpcClientError::RequestFailed {
				method: methods::STATE_CALL,
				message: e.to_string(),
			})
	}

	/// Get system properties (token decimals, symbols, etc.).
	pub async fn system_properties(
		&self,
	) -> Result<subxt::backend::legacy::rpc_methods::SystemProperties, RpcClientError> {
		self.legacy.read().await.system_properties().await.map_err(|e| {
			RpcClientError::RequestFailed {
				method: methods::SYSTEM_PROPERTIES,
				message: e.to_string(),
			}
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
		use crate::testing::{
			TestContext,
			constants::{SYSTEM_NUMBER_KEY, SYSTEM_PALLET_PREFIX, SYSTEM_PARENT_HASH_KEY},
		};
		use std::time::Duration;

		#[tokio::test]
		async fn connect_to_node() {
			let ctx = TestContext::for_rpc_client().await;
			assert_eq!(ctx.rpc().endpoint(), &ctx.endpoint);
		}

		#[tokio::test]
		async fn fetch_finalized_head() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();
			// Hash should be 32 bytes
			assert_eq!(hash.as_bytes().len(), 32);
		}

		#[tokio::test]
		async fn fetch_header() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();
			let header = ctx.rpc().header(hash).await.unwrap();
			// Header should have a valid state root (32 bytes)
			assert_eq!(header.state_root.as_bytes().len(), 32);
		}

		#[tokio::test]
		async fn fetch_storage() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();

			let key = hex::decode(SYSTEM_NUMBER_KEY).unwrap();
			let value = ctx.rpc().storage(&key, hash).await.unwrap();

			// System::Number should exist and have a value
			assert!(value.is_some());
		}

		#[tokio::test]
		async fn fetch_metadata() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();
			let metadata = ctx.rpc().metadata(hash).await.unwrap();

			// Decoded metadata should contain pallets
			assert!(metadata.pallets().len() > 0);
		}

		#[tokio::test]
		async fn fetch_runtime_code() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();
			let code = ctx.rpc().runtime_code(hash).await.unwrap();

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
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();

			let prefix = hex::decode(SYSTEM_PALLET_PREFIX).unwrap();
			let keys = ctx.rpc().storage_keys_paged(&prefix, 10, None, hash).await.unwrap();

			// Should find some System storage keys
			assert!(!keys.is_empty());
			// All keys should start with the prefix
			for key in &keys {
				assert!(key.starts_with(&prefix));
			}
		}

		#[tokio::test]
		async fn fetch_storage_batch() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();

			let keys = [
				hex::decode(SYSTEM_NUMBER_KEY).unwrap(),
				hex::decode(SYSTEM_PARENT_HASH_KEY).unwrap(),
			];
			let key_refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
			let values = ctx.rpc().storage_batch(&key_refs, hash).await.unwrap();

			assert_eq!(values.len(), 2);
			// Both System::Number and System::ParentHash should exist
			assert!(values[0].is_some());
			assert!(values[1].is_some());
		}

		#[tokio::test]
		async fn fetch_system_chain() {
			let ctx = TestContext::for_rpc_client().await;
			let chain_name = ctx.rpc().system_chain().await.unwrap();

			// Chain should return a non-empty name
			assert!(!chain_name.is_empty());
		}

		#[tokio::test]
		async fn fetch_system_properties() {
			let ctx = TestContext::for_rpc_client().await;

			// Just verify the call succeeds - ink-node may not have all standard properties
			let _properties = ctx.rpc().system_properties().await.unwrap();
		}

		#[tokio::test]
		async fn fetch_header_non_existent_block_fails() {
			let ctx = TestContext::for_rpc_client().await;

			// Use a fabricated block hash that doesn't exist
			let non_existent_hash = H256::from([0xde; 32]);
			let result = ctx.rpc().header(non_existent_hash).await;

			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(
				matches!(err, RpcClientError::InvalidResponse(_)),
				"Expected InvalidResponse for non-existent block, got: {err:?}"
			);
		}

		#[tokio::test]
		async fn fetch_storage_non_existent_key_returns_none() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();

			// Use a fabricated storage key that doesn't exist
			let non_existent_key = vec![0xff; 32];
			let result = ctx.rpc().storage(&non_existent_key, hash).await.unwrap();

			// Non-existent storage returns None, not an error
			assert!(result.is_none());
		}

		#[tokio::test]
		async fn fetch_storage_batch_with_mixed_keys() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();

			// Mix of existing and non-existing keys
			let keys = [
				hex::decode(SYSTEM_NUMBER_KEY).unwrap(), // exists
				vec![0xff; 32],                          // doesn't exist
			];
			let key_refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
			let values = ctx.rpc().storage_batch(&key_refs, hash).await.unwrap();

			assert_eq!(values.len(), 2);
			assert!(values[0].is_some(), "System::Number should exist");
			assert!(values[1].is_none(), "Fabricated key should not exist");
		}

		#[tokio::test]
		async fn fetch_storage_batch_empty_keys() {
			let ctx = TestContext::for_rpc_client().await;
			let hash = ctx.rpc().finalized_head().await.unwrap();

			// Empty keys should return empty results
			let values = ctx.rpc().storage_batch(&[], hash).await.unwrap();
			assert!(values.is_empty());
		}

		#[tokio::test]
		async fn fetch_block_by_number_returns_block() {
			let ctx = TestContext::for_rpc_client().await;

			// Get finalized block number by fetching the header first
			let finalized_hash = ctx.rpc().finalized_head().await.unwrap();
			let finalized_header = ctx.rpc().header(finalized_hash).await.unwrap();
			let finalized_number = finalized_header.number;

			// Fetch the block by number
			let result = ctx.rpc().block_by_number(finalized_number).await.unwrap();

			assert!(result.is_some(), "Finalized block should exist");
			let (hash, block) = result.unwrap();

			// Verify the hash matches the finalized head
			assert_eq!(hash, finalized_hash, "Block hash should match finalized head");

			// Verify the block has a header
			assert_eq!(
				block.header.number, finalized_number,
				"Block header number should match requested number"
			);
		}

		#[tokio::test]
		async fn fetch_block_by_number_non_existent_returns_none() {
			let ctx = TestContext::for_rpc_client().await;

			// Use a very large block number that doesn't exist
			let non_existent_number = u32::MAX;
			let result = ctx.rpc().block_by_number(non_existent_number).await.unwrap();

			assert!(result.is_none(), "Non-existent block should return None");
		}

		#[tokio::test]
		async fn fetch_block_by_number_multiple_blocks() {
			let ctx = TestContext::for_rpc_client().await;

			// Wait a bit to get some finalized blocks
			std::thread::sleep(Duration::from_secs(30));

			// Get finalized block number
			let finalized_hash = ctx.rpc().finalized_head().await.unwrap();
			let finalized_header = ctx.rpc().header(finalized_hash).await.unwrap();
			let finalized_number = finalized_header.number;

			// Fetch multiple blocks (0 to finalized)
			let mut previous_hash = None;
			for block_num in 0..=finalized_number.min(5) {
				let result = ctx.rpc().block_by_number(block_num).await.unwrap();
				assert!(
					result.is_some(),
					"Block {} should exist (finalized is {})",
					block_num,
					finalized_number
				);

				let (hash, block) = result.unwrap();
				assert_eq!(block.header.number, block_num);

				// Verify parent hash chain (except for genesis)
				if let Some(prev) = previous_hash {
					assert_eq!(
						block.header.parent_hash, prev,
						"Block {} parent hash should match previous block hash",
						block_num
					);
				}

				previous_hash = Some(hash);
			}
		}
	}
}
