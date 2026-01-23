// SPDX-License-Identifier: GPL-3.0

//! Legacy state_* RPC methods.
//!
//! These methods provide state-related operations for polkadot.js compatibility.

use crate::rpc_server::types::RuntimeVersion;
use crate::rpc_server::MockBlockchain;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use std::sync::Arc;

/// Legacy state RPC methods.
#[rpc(server, namespace = "state")]
pub trait StateApi {
	/// Get storage value at a key.
	///
	/// Returns the hex-encoded storage value at the given key, or null if no value exists.
	#[method(name = "getStorage")]
	async fn get_storage(&self, key: String, at: Option<String>) -> RpcResult<Option<String>>;

	/// Get the runtime metadata.
	///
	/// Returns the hex-encoded runtime metadata.
	#[method(name = "getMetadata")]
	async fn get_metadata(&self, at: Option<String>) -> RpcResult<String>;

	/// Get the runtime version.
	#[method(name = "getRuntimeVersion")]
	async fn get_runtime_version(&self, at: Option<String>) -> RpcResult<RuntimeVersion>;
}

/// Implementation of legacy state RPC methods.
pub struct StateApi {
	blockchain: Arc<MockBlockchain>,
}

impl StateApi {
	/// Create a new StateApi instance.
	pub fn new(blockchain: Arc<MockBlockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl StateApiServer for StateApi {
	async fn get_storage(&self, key: String, _at: Option<String>) -> RpcResult<Option<String>> {
		// Decode the hex key
		let key_bytes = hex::decode(key.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex key: {e}"),
				None::<()>,
			)
		})?;

		// Query storage from blockchain
		match self.blockchain.storage(&key_bytes).await {
			Ok(Some(value)) => Ok(Some(format!("0x{}", hex::encode(value)))),
			Ok(None) => Ok(None),
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Storage error: {e}"),
				None::<()>,
			)),
		}
	}

	async fn get_metadata(&self, _at: Option<String>) -> RpcResult<String> {
		// Mock: return empty metadata (would need real blockchain for actual metadata)
		// The actual metadata would be fetched via blockchain.call("Metadata_metadata", &[])
		Ok("0x".to_string())
	}

	async fn get_runtime_version(&self, _at: Option<String>) -> RpcResult<RuntimeVersion> {
		// Mock: return default runtime version
		Ok(RuntimeVersion::default())
	}
}
