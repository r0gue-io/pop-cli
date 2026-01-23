// SPDX-License-Identifier: GPL-3.0

//! Legacy state_* RPC methods.
//!
//! These methods provide state-related operations for polkadot.js compatibility.

use crate::rpc_server::types::RuntimeVersion;
use crate::Blockchain;
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
	blockchain: Arc<Blockchain>,
}

impl StateApi {
	/// Create a new StateApi instance.
	pub fn new(blockchain: Arc<Blockchain>) -> Self {
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
		// Fetch real metadata via runtime call
		match self.blockchain.call("Metadata_metadata", &[]).await {
			Ok(result) => Ok(format!("0x{}", hex::encode(result))),
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to get metadata: {e}"),
				None::<()>,
			)),
		}
	}

	async fn get_runtime_version(&self, _at: Option<String>) -> RpcResult<RuntimeVersion> {
		// Fetch runtime version via Core_version call and decode
		match self.blockchain.call("Core_version", &[]).await {
			Ok(result) => {
				// Decode the SCALE-encoded RuntimeVersion
				// Format: spec_name, impl_name, authoring_version, spec_version, impl_version,
				//         apis (Vec<([u8;8], u32)>), transaction_version, state_version
				use scale::Decode;

				#[derive(Decode)]
				struct ScaleRuntimeVersion {
					spec_name: String,
					impl_name: String,
					authoring_version: u32,
					spec_version: u32,
					impl_version: u32,
					apis: Vec<([u8; 8], u32)>,
					transaction_version: u32,
					state_version: u8,
				}

				let version =
					ScaleRuntimeVersion::decode(&mut result.as_slice()).map_err(|e| {
						jsonrpsee::types::ErrorObjectOwned::owned(
							-32603,
							format!("Failed to decode runtime version: {e}"),
							None::<()>,
						)
					})?;

				Ok(RuntimeVersion {
					spec_name: version.spec_name,
					impl_name: version.impl_name,
					authoring_version: version.authoring_version,
					spec_version: version.spec_version,
					impl_version: version.impl_version,
					transaction_version: version.transaction_version,
					state_version: version.state_version,
					apis: version
						.apis
						.into_iter()
						.map(|(id, ver)| (format!("0x{}", hex::encode(id)), ver))
						.collect(),
				})
			},
			Err(e) => Err(jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to get runtime version: {e}"),
				None::<()>,
			)),
		}
	}
}
