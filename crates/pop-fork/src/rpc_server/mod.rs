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
use std::{net::SocketAddr, sync::Arc};
use subxt::config::substrate::H256;

/// Parse a hex-encoded string into an H256 block hash.
pub fn parse_block_hash(hex: &str) -> Result<H256, RpcServerError> {
	let bytes = hex::decode(hex.trim_start_matches("0x"))
		.map_err(|e| RpcServerError::InvalidParam(format!("Invalid hex hash: {e}")))?;
    if bytes.len() != 32{
        return Err(RpcServerError::Internal("Invalid block hash length."));
    }
	Ok(H256::from_slice(&bytes))
}

/// Parse a hex-encoded string into raw bytes.
pub fn parse_hex_bytes(hex: &str, field_name: &str) -> Result<Vec<u8>, RpcServerError> {
	hex::decode(hex.trim_start_matches("0x"))
		.map_err(|e| RpcServerError::InvalidParam(format!("Invalid hex {field_name}: {e}")))
}

/// Default starting port for the RPC server.
pub const DEFAULT_RPC_PORT: u16 = 9944;

/// Maximum port to try when auto-finding an available port.
const MAX_PORT_ATTEMPTS: u16 = 20;

/// Configuration for the RPC server.
#[derive(Debug, Clone)]
pub struct RpcServerConfig {
	/// Port to bind the server to. If `None`, starts at `DEFAULT_RPC_PORT` and auto-increments
	/// until finding an available port, falling back to a random available port if needed.
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
	/// If `config.port` is `None`, starts at `DEFAULT_RPC_PORT` and auto-increments until
	/// finding an available port, falling back to a random available port if needed.
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
			let mut found = None;

			for port in DEFAULT_RPC_PORT..DEFAULT_RPC_PORT.saturating_add(MAX_PORT_ATTEMPTS) {
				let addr: SocketAddr = ([127, 0, 0, 1], port).into();
				if let Ok(server) = ServerBuilder::default()
					.max_connections(config.max_connections)
					.build(addr)
					.await
				{
					let bound_addr = server
						.local_addr()
						.map_err(|e| RpcServerError::ServerStart(e.to_string()))?;
					found = Some((server, bound_addr));
					break;
				}
			}

			// If no port in the preferred range is available, use a random available port
			match found {
				Some(result) => result,
				None => {
					let port = pop_common::resolve_port(None);
					let addr: SocketAddr = ([127, 0, 0, 1], port).into();
					let server = ServerBuilder::default()
						.max_connections(config.max_connections)
						.build(addr)
						.await
						.map_err(|e| RpcServerError::ServerStart(e.to_string()))?;
					let bound_addr = server
						.local_addr()
						.map_err(|e| RpcServerError::ServerStart(e.to_string()))?;
					(server, bound_addr)
				},
			}
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
