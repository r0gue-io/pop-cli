// SPDX-License-Identifier: GPL-3.0

//! New chainHead_v1_* RPC methods.
//!
//! These methods follow the new Substrate JSON-RPC specification for chain head tracking.
//! Note: Subscriptions are stubbed for now - full implementation in follow-up PR.

use crate::rpc_server::types::{MethodResponse, OperationResult, StorageQueryItem};
use crate::rpc_server::MockBlockchain;
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use std::sync::Arc;

/// New chainHead RPC methods (v1 spec).
#[rpc(server, namespace = "chainHead")]
pub trait ChainHeadApi {
	/// Get the header of a pinned block.
	#[method(name = "v1_header")]
	async fn header(&self, follow_subscription: String, hash: String) -> RpcResult<Option<String>>;

	/// Execute a runtime call at a pinned block.
	#[method(name = "v1_call")]
	async fn call(
		&self,
		follow_subscription: String,
		hash: String,
		function: String,
		call_parameters: String,
	) -> RpcResult<MethodResponse>;

	/// Query storage at a pinned block.
	#[method(name = "v1_storage")]
	async fn storage(
		&self,
		follow_subscription: String,
		hash: String,
		items: Vec<StorageQueryItem>,
		child_trie: Option<String>,
	) -> RpcResult<MethodResponse>;

	/// Get the body of a pinned block.
	#[method(name = "v1_body")]
	async fn body(&self, follow_subscription: String, hash: String) -> RpcResult<MethodResponse>;

	/// Continue a paused operation.
	#[method(name = "v1_continue")]
	async fn continue_op(
		&self,
		follow_subscription: String,
		operation_id: String,
	) -> RpcResult<()>;

	/// Stop an operation.
	#[method(name = "v1_stopOperation")]
	async fn stop_operation(
		&self,
		follow_subscription: String,
		operation_id: String,
	) -> RpcResult<()>;

	/// Unpin one or more blocks.
	#[method(name = "v1_unpin")]
	async fn unpin(
		&self,
		follow_subscription: String,
		hash_or_hashes: serde_json::Value,
	) -> RpcResult<()>;
}

/// Implementation of chainHead RPC methods.
pub struct ChainHeadApi {
	#[allow(dead_code)]
	blockchain: Arc<MockBlockchain>,
}

impl ChainHeadApi {
	/// Create a new ChainHeadApi instance.
	pub fn new(blockchain: Arc<MockBlockchain>) -> Self {
		Self { blockchain }
	}
}

#[async_trait::async_trait]
impl ChainHeadApiServer for ChainHeadApi {
	async fn header(
		&self,
		_follow_subscription: String,
		_hash: String,
	) -> RpcResult<Option<String>> {
		// Mock: return None (would need subscription tracking for real impl)
		Ok(None)
	}

	async fn call(
		&self,
		_follow_subscription: String,
		_hash: String,
		_function: String,
		_call_parameters: String,
	) -> RpcResult<MethodResponse> {
		// Mock: return a started operation
		Ok(MethodResponse {
			result: OperationResult::Started {
				operation_id: "mock-operation-1".to_string(),
			},
		})
	}

	async fn storage(
		&self,
		_follow_subscription: String,
		_hash: String,
		_items: Vec<StorageQueryItem>,
		_child_trie: Option<String>,
	) -> RpcResult<MethodResponse> {
		// Mock: return a started operation
		Ok(MethodResponse {
			result: OperationResult::Started {
				operation_id: "mock-operation-2".to_string(),
			},
		})
	}

	async fn body(
		&self,
		_follow_subscription: String,
		_hash: String,
	) -> RpcResult<MethodResponse> {
		// Mock: return a started operation
		Ok(MethodResponse {
			result: OperationResult::Started {
				operation_id: "mock-operation-3".to_string(),
			},
		})
	}

	async fn continue_op(
		&self,
		_follow_subscription: String,
		_operation_id: String,
	) -> RpcResult<()> {
		// Mock: no-op
		Ok(())
	}

	async fn stop_operation(
		&self,
		_follow_subscription: String,
		_operation_id: String,
	) -> RpcResult<()> {
		// Mock: no-op
		Ok(())
	}

	async fn unpin(
		&self,
		_follow_subscription: String,
		_hash_or_hashes: serde_json::Value,
	) -> RpcResult<()> {
		// Mock: no-op
		Ok(())
	}
}
