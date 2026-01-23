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
pub mod methods;
pub mod types;

pub use error::{RpcServerError, error_codes};

use crate::{Blockchain, TxPool};

use jsonrpsee::server::{ServerBuilder, ServerHandle};
use std::net::SocketAddr;
use std::sync::Arc;

/// Default starting port for the RPC server.
pub const DEFAULT_RPC_PORT: u16 = 8000;

/// Maximum port to try when auto-finding an available port.
const MAX_PORT_ATTEMPTS: u16 = 100;

/// Configuration for the RPC server.
#[derive(Debug, Clone)]
pub struct RpcServerConfig {
	/// Port to bind the server to. If `None`, starts at 8000 and auto-increments
	/// until finding an available port.
	pub port: Option<u16>,
	/// Maximum number of connections.
	pub max_connections: u32,
}

impl Default for RpcServerConfig {
	fn default() -> Self {
		Self { port: None, max_connections: 100 }
	}
}

impl RpcServerConfig {
	/// Create a config with a specific port.
	pub fn with_port(port: u16) -> Self {
		Self { port: Some(port), max_connections: 100 }
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
	/// Start a new RPC server with the given blockchain, transaction pool, and configuration.
	///
	/// If `config.port` is `Some(port)`, attempts to bind to that specific port.
	/// If `config.port` is `None`, starts at port 8000 and auto-increments until
	/// finding an available port (useful when forking multiple chains).
	pub async fn start(
		blockchain: Arc<Blockchain>,
		txpool: Arc<TxPool>,
		config: RpcServerConfig,
	) -> Result<Self, RpcServerError> {
		// Create RPC module first (doesn't need the server)
		let rpc_module = methods::create_rpc_module(blockchain, txpool)?;

		let (server, addr) = if let Some(port) = config.port {
			// User specified a port - try only that one
			let addr: SocketAddr = ([127, 0, 0, 1], port).into();
			let server = ServerBuilder::default()
				.max_connections(config.max_connections)
				.build(addr)
				.await
				.map_err(|e| RpcServerError::ServerStart(e.to_string()))?;
			let addr =
				server.local_addr().map_err(|e| RpcServerError::ServerStart(e.to_string()))?;
			(server, addr)
		} else {
			// Auto-find an available port starting from DEFAULT_RPC_PORT
			let mut last_error = None;
			let mut found = None;

			for port in DEFAULT_RPC_PORT..DEFAULT_RPC_PORT.saturating_add(MAX_PORT_ATTEMPTS) {
				let addr: SocketAddr = ([127, 0, 0, 1], port).into();
				match ServerBuilder::default()
					.max_connections(config.max_connections)
					.build(addr)
					.await
				{
					Ok(server) => {
						let bound_addr = server
							.local_addr()
							.map_err(|e| RpcServerError::ServerStart(e.to_string()))?;
						found = Some((server, bound_addr));
						break;
					},
					Err(e) => {
						last_error = Some(e);
						continue;
					},
				}
			}

			found.ok_or_else(|| {
				RpcServerError::ServerStart(format!(
					"Could not find available port in range {}-{}: {}",
					DEFAULT_RPC_PORT,
					DEFAULT_RPC_PORT.saturating_add(MAX_PORT_ATTEMPTS),
					last_error.map(|e| e.to_string()).unwrap_or_default()
				))
			})?
		};

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
