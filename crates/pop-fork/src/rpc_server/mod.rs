// SPDX-License-Identifier: GPL-3.0

//! JSON-RPC server for forked blockchain.
//!
//! This module provides a Substrate-compatible JSON-RPC server that exposes
//! the forked blockchain state to external tools like polkadot.js.
//!
//! # Supported RPC Methods
//!
//! ## Legacy Methods (for polkadot.js compatibility)
//! - `chain_*` - Block operations (getBlockHash, getHeader, getBlock, getFinalizedHead)
//! - `state_*` - State operations (getStorage, getMetadata, getRuntimeVersion)
//! - `system_*` - System info (chain, name, version, health, properties)
//! - `author_*` - Transaction submission (submitExtrinsic, pendingExtrinsics)
//!
//! ## New Substrate RPC Specs
//! - `chainHead_v1_*` - Modern chain head tracking with subscriptions
//! - `archive_v1_*` - Archive node queries
//! - `transaction_v1_*` - Transaction broadcasting

mod error;
mod mock_blockchain;
pub mod methods;
pub mod types;

pub use error::{RpcServerError, error_codes};
pub use mock_blockchain::MockBlockchain;

use jsonrpsee::server::{ServerBuilder, ServerHandle};
use std::net::SocketAddr;
use std::sync::Arc;

/// Configuration for the RPC server.
#[derive(Debug, Clone)]
pub struct RpcServerConfig {
	/// Address to bind the server to.
	pub addr: SocketAddr,
	/// Maximum number of connections.
	pub max_connections: u32,
}

impl Default for RpcServerConfig {
	fn default() -> Self {
		Self {
			addr: ([127, 0, 0, 1], 8000).into(),
			max_connections: 100,
		}
	}
}

/// The RPC server for a forked blockchain.
pub struct ForkRpcServer {
	/// Server handle for managing lifecycle.
	handle: ServerHandle,
	/// Address the server is bound to.
	addr: SocketAddr,
}

impl ForkRpcServer {
	/// Start a new RPC server with the given blockchain and configuration.
	pub async fn start(
		blockchain: Arc<MockBlockchain>,
		config: RpcServerConfig,
	) -> Result<Self, RpcServerError> {
		let server = ServerBuilder::default()
			.max_connections(config.max_connections)
			.build(config.addr)
			.await
			.map_err(|e| RpcServerError::ServerStart(e.to_string()))?;

		let addr = server.local_addr().map_err(|e| RpcServerError::ServerStart(e.to_string()))?;

		// Merge all RPC methods into a single module
		let rpc_module = methods::create_rpc_module(blockchain)?;

		let handle = server.start(rpc_module);

		Ok(Self { handle, addr })
	}

	/// Get the address the server is bound to.
	pub fn addr(&self) -> SocketAddr {
		self.addr
	}

	/// Get the WebSocket URL for connecting to this server.
	pub fn ws_url(&self) -> String {
		format!("ws://{}", self.addr)
	}

	/// Get the HTTP URL for connecting to this server.
	pub fn http_url(&self) -> String {
		format!("http://{}", self.addr)
	}

	/// Stop the server gracefully.
	pub async fn stop(self) {
		self.handle.stop().expect("Server stop should not fail");
		self.handle.stopped().await;
	}

	/// Get a handle to check if the server is still running.
	pub fn handle(&self) -> &ServerHandle {
		&self.handle
	}
}
