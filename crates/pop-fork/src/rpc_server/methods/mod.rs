// SPDX-License-Identifier: GPL-3.0

//! RPC method implementations.
//!
//! This module contains all RPC method implementations organized by namespace:
//! - `chain` - Legacy chain_* methods
//! - `state` - Legacy state_* methods
//! - `system` - Legacy system_* methods
//! - `author` - Legacy author_* methods
//! - `payment` - Fee estimation methods (queryInfo, queryFeeDetails)
//! - `archive` - New archive_v1_* methods
//! - `chain_head` - New chainHead_v1_* methods (PAPI compatibility)
//! - `chain_spec` - New chainSpec_v1_* methods
//! - `transaction` - New transaction_v1_* methods
//! - `dev` - Development methods for manual chain control

mod archive;
mod author;
mod chain;
mod chain_head;
mod chain_spec;
mod dev;
mod payment;
mod state;
mod system;
mod transaction;

use crate::{Blockchain, TxPool, rpc_server::RpcServerError};
use jsonrpsee::{RpcModule, types::ResponsePayload};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub use archive::{ArchiveApi, ArchiveApiServer};
pub use author::{AuthorApi, AuthorApiServer};
pub use chain::{ChainApi, ChainApiServer};
pub use chain_head::{ChainHeadApi, ChainHeadApiServer, ChainHeadState};
pub use chain_spec::{ChainSpecApi, ChainSpecApiServer};
pub use dev::{DevApi, DevApiServer};
pub use payment::{PaymentApi, PaymentApiServer};
pub use state::{StateApi, StateApiServer};
pub use system::{SystemApi, SystemApiServer};
pub use transaction::{TransactionApi, TransactionApiServer};

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
	shutdown_token: CancellationToken,
) -> Result<RpcModule<()>, RpcServerError> {
	let mut module = RpcModule::new(());

	// Create implementations
	let chain_impl = ChainApi::new(blockchain.clone(), shutdown_token.clone());
	let state_impl = StateApi::new(blockchain.clone(), shutdown_token.clone());
	let system_impl = SystemApi::new(blockchain.clone());
	let author_impl = AuthorApi::new(blockchain.clone(), txpool.clone());
	let archive_impl = ArchiveApi::new(blockchain.clone());
	let chain_head_state = Arc::new(ChainHeadState::new());
	let chain_head_impl = ChainHeadApi::new(blockchain.clone(), chain_head_state, shutdown_token);
	let chain_spec_impl = ChainSpecApi::new(blockchain.clone());
	let payment_impl = PaymentApi::new(blockchain.clone());
	let transaction_impl = TransactionApi::new(blockchain.clone(), txpool.clone());
	let dev_impl = DevApi::new(blockchain, txpool);

	// Merge all methods into the module
	module
		.merge(ChainApiServer::into_rpc(chain_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	// Register subscription aliases for polkadot.js compatibility
	// (singular vs plural naming: subscribeNewHead vs subscribeNewHeads)
	module
		.register_alias("chain_subscribeNewHead", "chain_subscribeNewHeads")
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;
	module
		.register_alias("chain_unsubscribeNewHead", "chain_unsubscribeNewHeads")
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
		.merge(ChainSpecApiServer::into_rpc(chain_spec_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(PaymentApiServer::into_rpc(payment_impl))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	module
		.merge(TransactionApiServer::into_rpc(transaction_impl))
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
		.register_method("rpc_methods", move |_, _, _| ResponsePayload::success(response.clone()))
		.map_err(|e| RpcServerError::Internal(e.to_string()))?;

	Ok(module)
}
