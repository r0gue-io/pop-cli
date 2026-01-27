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
//! - `dev` - Development methods for manual chain control

mod archive;
mod author;
mod chain;
mod chain_head;
mod dev;
mod state;
mod system;
mod transaction;

use crate::{Blockchain, TxPool, rpc_server::RpcServerError};
use jsonrpsee::RpcModule;
use std::sync::Arc;

pub use archive::{ArchiveApi, ArchiveApiServer};
pub use author::{AuthorApi, AuthorApiServer};
pub use chain::{ChainApi, ChainApiServer};
pub use chain_head::{ChainHeadApi, ChainHeadApiServer};
pub use dev::{DevApi, DevApiServer};
pub use state::{StateApi, StateApiServer};
pub use system::{SystemApi, SystemApiServer};
pub use transaction::{TransactionApi, TransactionApiServer};

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
	let chain_head_impl = ChainHeadApi::new(blockchain.clone());
	let transaction_impl = TransactionApi::new(blockchain.clone(), txpool.clone());
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
		.merge(ChainHeadApiServer::into_rpc(chain_head_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(TransactionApiServer::into_rpc(transaction_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(DevApiServer::into_rpc(dev_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	Ok(module)
}
