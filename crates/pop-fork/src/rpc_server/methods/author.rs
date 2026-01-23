// SPDX-License-Identifier: GPL-3.0

//! Legacy author_* RPC methods.
//!
//! These methods provide transaction submission for polkadot.js compatibility.

use crate::rpc_server::MockBlockchain;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use std::sync::Arc;

/// Legacy author RPC methods.
#[rpc(server, namespace = "author")]
pub trait AuthorApi {
	/// Submit a fully formatted extrinsic for block inclusion.
	///
	/// Returns the hash of the submitted extrinsic.
	#[method(name = "submitExtrinsic")]
	async fn submit_extrinsic(&self, extrinsic: String) -> RpcResult<String>;

	/// Get all pending extrinsics.
	#[method(name = "pendingExtrinsics")]
	async fn pending_extrinsics(&self) -> RpcResult<Vec<String>>;
}

/// Implementation of legacy author RPC methods.
pub struct AuthorApi {
	#[allow(dead_code)]
	blockchain: Arc<MockBlockchain>,
}

impl AuthorApi {
	/// Create a new AuthorApi instance.
	pub fn new(blockchain: Arc<MockBlockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl AuthorApiServer for AuthorApi {
	async fn submit_extrinsic(&self, extrinsic: String) -> RpcResult<String> {
		// Decode the hex extrinsic
		let _ext_bytes = hex::decode(extrinsic.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex extrinsic: {e}"),
				None::<()>,
			)
		})?;

		// Mock: Would submit to TxPool and return hash
		// For now, return a mock hash
		let mock_hash = sp_core::blake2_256(&_ext_bytes);
		Ok(format!("0x{}", hex::encode(mock_hash)))
	}

	async fn pending_extrinsics(&self) -> RpcResult<Vec<String>> {
		// Mock: return empty list (no TxPool integration yet)
		Ok(vec![])
	}
}
