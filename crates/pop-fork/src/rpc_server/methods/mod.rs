// SPDX-License-Identifier: GPL-3.0

//! RPC method implementations.
//!
//! This module contains all RPC method implementations organized by namespace:
//! - `chain` - Legacy chain_* methods
//! - `state` - Legacy state_* methods
//! - `system` - Legacy system_* methods
//! - `author` - Legacy author_* methods
//! - `archive` - New archive_v1_* methods
//! - `chain_head` - New chainHead_v1_* methods
//! - `transaction` - New transaction_v1_* methods

mod archive;
mod author;
mod chain;
mod chain_head;
mod state;
mod system;
mod transaction;

use crate::rpc_server::{MockBlockchain, RpcServerError};
use jsonrpsee::RpcModule;
use std::sync::Arc;

pub use archive::ArchiveApiServer;
pub use author::AuthorApiServer;
pub use chain::ChainApiServer;
pub use chain_head::ChainHeadApiServer;
pub use state::StateApiServer;
pub use system::SystemApiServer;
pub use transaction::TransactionApiServer;

/// Create the merged RPC module with all methods.
pub fn create_rpc_module(
	blockchain: Arc<MockBlockchain>,
) -> Result<RpcModule<()>, RpcServerError> {
	let mut module = RpcModule::new(());

	// Create implementations
	let chain_impl = chain::ChainApi::new(blockchain.clone());
	let state_impl = state::StateApi::new(blockchain.clone());
	let system_impl = system::SystemApi::new(blockchain.clone());
	let author_impl = author::AuthorApi::new(blockchain.clone());
	let archive_impl = archive::ArchiveApi::new(blockchain.clone());
	let chain_head_impl = chain_head::ChainHeadApi::new(blockchain.clone());
	let transaction_impl = transaction::TransactionApi::new(blockchain);

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
		.merge(ChainHeadApiServer::into_rpc(chain_head_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(TransactionApiServer::into_rpc(transaction_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	Ok(module)
}
