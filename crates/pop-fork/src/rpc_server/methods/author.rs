// SPDX-License-Identifier: GPL-3.0

//! Legacy author_* RPC methods.
//!
//! These methods provide transaction submission for polkadot.js compatibility.

use crate::{Blockchain, TxPool};
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
	blockchain: Arc<Blockchain>,
	txpool: Arc<TxPool>,
}

impl AuthorApi {
	/// Create a new AuthorApi instance.
	pub fn new(blockchain: Arc<Blockchain>, txpool: Arc<TxPool>) -> Self {
		Self { blockchain, txpool }
	}
}

#[async_trait::async_trait]
impl AuthorApiServer for AuthorApi {
	async fn submit_extrinsic(&self, extrinsic: String) -> RpcResult<String> {
		// Decode the hex extrinsic
		let ext_bytes = hex::decode(extrinsic.trim_start_matches("0x")).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32602,
				format!("Invalid hex extrinsic: {e}"),
				None::<()>,
			)
		})?;

		// Submit to TxPool and return hash
		let hash = self.txpool.submit(ext_bytes).map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to submit extrinsic: {e}"),
				None::<()>,
			)
		})?;
		Ok(format!("0x{}", hex::encode(hash.as_bytes())))
	}

	async fn pending_extrinsics(&self) -> RpcResult<Vec<String>> {
		let pending = self.txpool.pending().map_err(|e| {
			jsonrpsee::types::ErrorObjectOwned::owned(
				-32603,
				format!("Failed to get pending extrinsics: {e}"),
				None::<()>,
			)
		})?;
		Ok(pending.iter().map(|ext| format!("0x{}", hex::encode(ext))).collect())
	}
}
