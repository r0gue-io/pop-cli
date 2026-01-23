// SPDX-License-Identifier: GPL-3.0

//! Legacy system_* RPC methods.
//!
//! These methods provide system information for polkadot.js compatibility.

use crate::rpc_server::types::{ChainProperties, SystemHealth};
use crate::Blockchain;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
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
	async fn properties(&self) -> RpcResult<ChainProperties>;
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
		Ok(env!("CARGO_PKG_VERSION").to_string())
	}

	async fn health(&self) -> RpcResult<SystemHealth> {
		// Fork is always "healthy" - no syncing, no peers needed
		Ok(SystemHealth::default())
	}

	async fn properties(&self) -> RpcResult<ChainProperties> {
		// Mock: return empty properties (would fetch from chain in real impl)
		Ok(ChainProperties::default())
	}
}
