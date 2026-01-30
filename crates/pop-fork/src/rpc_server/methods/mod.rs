// SPDX-License-Identifier: GPL-3.0

//! RPC method implementations.
//!
//! This module contains all RPC method implementations organized by namespace:
//! - `chain` - Legacy chain_* methods
//! - `state` - Legacy state_* methods
//! - `system` - Legacy system_* methods
//! - `author` - Legacy author_* methods
//! - `archive` - New archive_v1_* methods
//! - `chain_spec` - New chainSpec_v1_* methods
//! - `transaction` - New transaction_v1_* methods
//! - `dev` - Development methods for manual chain control

mod archive;
mod author;
mod chain;
mod chain_spec;
mod dev;
mod state;
mod system;

use crate::{Blockchain, TxPool, rpc_server::RpcServerError};
use jsonrpsee::{RpcModule, types::ResponsePayload};
use std::sync::Arc;

pub use archive::{ArchiveApi, ArchiveApiServer};
pub use author::{AuthorApi, AuthorApiServer};
pub use chain::{ChainApi, ChainApiServer};
pub use chain_spec::{ChainSpecApi, ChainSpecApiServer};
pub use dev::{DevApi, DevApiServer};
pub use state::{StateApi, StateApiServer};
pub use system::{SystemApi, SystemApiServer};

/// Response for the `rpc_methods` RPC call.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RpcMethodsResponse {
	/// List of available RPC methods.
	pub methods: Vec<String>,
}

/// Create the merged RPC module with all methods.
pub fn create_rpc_module(
	blockchain: Arc<Blockchain>,
	txpool: Arc<TxPool>,
) -> Result<RpcModule<()>, RpcServerError> {
	let mut module = RpcModule::new(());

	// Create implementations
	let chain_impl = ChainApi::new(blockchain.clone());
	let state_impl = StateApi::new(blockchain.clone());
	let system_impl = SystemApi::new(blockchain.clone());
	let author_impl = AuthorApi::new(blockchain.clone(), txpool.clone());
	let archive_impl = ArchiveApi::new(blockchain.clone());
	let chain_spec_impl = ChainSpecApi::new(blockchain.clone());
	let dev_impl = DevApi::new(blockchain, txpool);

	// Merge all methods into the module
	module
		.merge(ChainApiServer::into_rpc(chain_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(StateApiServer::into_rpc(state_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(SystemApiServer::into_rpc(system_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(AuthorApiServer::into_rpc(author_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(ArchiveApiServer::into_rpc(archive_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(ChainSpecApiServer::into_rpc(chain_spec_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(DevApiServer::into_rpc(dev_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	// Collect method names before registering rpc_methods
	let mut method_names: Vec<String> = module.method_names().map(String::from).collect();
	method_names.push("rpc_methods".to_string());
	method_names.sort();

	// Register rpc_methods
	let response = RpcMethodsResponse { methods: method_names };
	module
		.register_method("rpc_methods", move |_, _, _| {
			ResponsePayload::success(response.clone())
		})
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	Ok(module)
}
