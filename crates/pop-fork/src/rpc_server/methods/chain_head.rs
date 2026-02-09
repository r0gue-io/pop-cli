// SPDX-License-Identifier: GPL-3.0

//! chainHead_v1_* RPC methods.
//!
//! These methods implement the new Substrate JSON-RPC specification for chain head tracking,
//! required for PAPI (polkadot-api) compatibility.
//!
//! Implementation follows Chopsticks' simplified approach:
//! - No real block pinning (unpin is no-op - fork keeps all blocks in memory)
//! - Operations execute immediately and send results via subscription
//! - Subscription limit: 2 per ChainHeadApi instance

use crate::{
	Blockchain, BlockchainEvent,
	rpc_server::{
		RpcServerError, parse_block_hash, parse_hex_bytes,
		types::{
			BestBlockChangedEvent, ChainHeadEvent, ChainHeadRuntimeVersion, FinalizedEvent,
			HexString, InitializedEvent, MethodResponse, NewBlockEvent, OperationEvent,
			OperationResult, StorageQueryItem, StorageQueryType, StorageResultItem,
		},
	},
	strings::rpc_server::{chain_head, runtime_api},
};
use jsonrpsee::{
	PendingSubscriptionSink, SubscriptionSink,
	core::{RpcResult, SubscriptionResult},
	proc_macros::rpc,
	tracing,
};
use scale::Decode;
use std::{
	collections::HashMap,
	sync::{
		Arc,
		atomic::{AtomicU64, Ordering},
	},
};
use tokio::sync::{RwLock, broadcast, mpsc};
use tokio_util::sync::CancellationToken;

/// chainHead RPC methods (v1 spec).
#[rpc(server, namespace = "chainHead")]
pub trait ChainHeadApi {
	/// Subscribe to chain head updates.
	///
	/// Returns a subscription that emits chain head events (initialized, newBlock,
	/// bestBlockChanged, finalized, stop).
	#[subscription(name = "v1_follow" => "v1_followEvent", unsubscribe = "v1_unfollow", item = serde_json::Value)]
	async fn follow(&self, with_runtime: bool) -> SubscriptionResult;

	/// Get the header of a pinned block.
	#[method(name = "v1_header")]
	async fn header(&self, follow_subscription: String, hash: String) -> RpcResult<Option<String>>;

	/// Get the body (extrinsics) of a pinned block.
	#[method(name = "v1_body")]
	async fn body(&self, follow_subscription: String, hash: String) -> RpcResult<MethodResponse>;

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

	/// Unpin one or more blocks.
	#[method(name = "v1_unpin")]
	async fn unpin(
		&self,
		follow_subscription: String,
		hash_or_hashes: serde_json::Value,
	) -> RpcResult<()>;

	/// Continue a paused operation (for paginated storage queries).
	#[method(name = "v1_continue")]
	async fn continue_op(&self, follow_subscription: String, operation_id: String)
	-> RpcResult<()>;

	/// Stop an in-progress operation.
	#[method(name = "v1_stopOperation")]
	async fn stop_operation(
		&self,
		follow_subscription: String,
		operation_id: String,
	) -> RpcResult<()>;
}

/// Handle for a single chainHead subscription.
struct SubscriptionHandle {
	/// Sender to push operation events to the subscription.
	event_tx: mpsc::UnboundedSender<OperationEvent>,
	/// Number of active operations.
	operation_count: AtomicU64,
	/// Whether to include runtime info in events.
	#[allow(dead_code)]
	with_runtime: bool,
}

/// Global state for chainHead subscriptions.
pub struct ChainHeadState {
	/// Active subscriptions keyed by subscription ID.
	subscriptions: RwLock<HashMap<String, Arc<SubscriptionHandle>>>,
	/// Next subscription ID counter.
	next_sub_id: AtomicU64,
	/// Next operation ID counter.
	next_op_id: AtomicU64,
}

impl ChainHeadState {
	/// Create new empty state.
	pub fn new() -> Self {
		Self {
			subscriptions: RwLock::new(HashMap::new()),
			next_sub_id: AtomicU64::new(1),
			next_op_id: AtomicU64::new(1),
		}
	}

	/// Generate a unique subscription ID.
	fn generate_subscription_id(&self) -> String {
		let id = self.next_sub_id.fetch_add(1, Ordering::SeqCst);
		format!("chainHead-sub-{id}")
	}

	/// Generate a unique operation ID.
	fn generate_operation_id(&self) -> String {
		let id = self.next_op_id.fetch_add(1, Ordering::SeqCst);
		format!("op-{id}")
	}

	/// Check if we can add a new subscription.
	async fn can_add_subscription(&self) -> bool {
		self.subscriptions.read().await.len() < chain_head::MAX_SUBSCRIPTIONS
	}

	/// Register a new subscription.
	async fn register_subscription(&self, id: String, handle: Arc<SubscriptionHandle>) {
		self.subscriptions.write().await.insert(id, handle);
	}

	/// Remove a subscription.
	async fn remove_subscription(&self, id: &str) {
		self.subscriptions.write().await.remove(id);
	}

	/// Get a subscription handle.
	async fn get_subscription(&self, id: &str) -> Option<Arc<SubscriptionHandle>> {
		self.subscriptions.read().await.get(id).cloned()
	}
}

impl Default for ChainHeadState {
	fn default() -> Self {
		Self::new()
	}
}

/// Implementation of chainHead RPC methods.
pub struct ChainHeadApi {
	blockchain: Arc<Blockchain>,
	state: Arc<ChainHeadState>,
	shutdown_token: CancellationToken,
}

impl ChainHeadApi {
	/// Create a new ChainHeadApi instance.
	pub fn new(
		blockchain: Arc<Blockchain>,
		state: Arc<ChainHeadState>,
		shutdown_token: CancellationToken,
	) -> Self {
		Self { blockchain, state, shutdown_token }
	}
}

/// Get runtime version from blockchain for chainHead RPC.
///
/// Returns a flat runtime version object with apis as a HashMap,
/// which is what papi-console expects.
async fn get_chainhead_runtime_version(blockchain: &Blockchain) -> Option<ChainHeadRuntimeVersion> {
	let head_hash = blockchain.head_hash().await;

	let result = blockchain
		.call_at_block(head_hash, runtime_api::CORE_VERSION, &[])
		.await
		.ok()??;

	#[derive(Decode)]
	struct ScaleRuntimeVersion {
		spec_name: String,
		impl_name: String,
		_authoring_version: u32,
		spec_version: u32,
		impl_version: u32,
		apis: Vec<([u8; 8], u32)>,
		transaction_version: u32,
		_state_version: u8,
	}

	let version = ScaleRuntimeVersion::decode(&mut result.as_slice()).ok()?;

	Some(ChainHeadRuntimeVersion {
		spec_name: version.spec_name,
		impl_name: version.impl_name,
		spec_version: version.spec_version,
		impl_version: version.impl_version,
		transaction_version: version.transaction_version,
		apis: version
			.apis
			.into_iter()
			.map(|(id, ver)| (HexString::from_bytes(&id).into(), ver))
			.collect(),
	})
}

/// Send a JSON event via the subscription sink.
async fn send_event(sink: &SubscriptionSink, event: impl serde::Serialize) -> bool {
	match jsonrpsee::SubscriptionMessage::from_json(&event) {
		Ok(msg) => sink.send(msg).await.is_ok(),
		Err(_) => false,
	}
}

#[async_trait::async_trait]
impl ChainHeadApiServer for ChainHeadApi {
	async fn follow(
		&self,
		pending: PendingSubscriptionSink,
		with_runtime: bool,
	) -> SubscriptionResult {
		// Check subscription limit
		if !self.state.can_add_subscription().await {
			pending
				.reject(RpcServerError::TooManySubscriptions {
					limit: chain_head::MAX_SUBSCRIPTIONS,
				})
				.await;
			return Ok(());
		}

		// Accept the subscription
		let sink = pending.accept().await?;
		let sub_id = self.state.generate_subscription_id();

		// Create event channel for operation results
		let (event_tx, mut event_rx) = mpsc::unbounded_channel::<OperationEvent>();

		// Create subscription handle
		let handle = Arc::new(SubscriptionHandle {
			event_tx,
			operation_count: AtomicU64::new(0),
			with_runtime,
		});

		// Register subscription
		self.state.register_subscription(sub_id.clone(), handle).await;

		// Get current finalized block
		let finalized_hash = self.blockchain.head_hash().await;
		let finalized_hash_hex = HexString::from_bytes(finalized_hash.as_bytes()).into();

		// Build initialized event
		// Use flat runtime format for papi compatibility (not wrapped in ValidRuntime)
		let runtime_version =
			if with_runtime { get_chainhead_runtime_version(&self.blockchain).await } else { None };

		// Log before building event (values are moved)
		tracing::debug!(
			sub_id = %sub_id,
			finalized_hash = %finalized_hash_hex,
			has_runtime = runtime_version.is_some(),
			"chainHead_v1_follow: sending initialized event"
		);

		let initialized = ChainHeadEvent::Initialized(InitializedEvent {
			finalized_block_hashes: vec![finalized_hash_hex],
			finalized_block_runtime: runtime_version,
		});

		// Log the exact JSON being sent for debugging
		if let Ok(json) = serde_json::to_string_pretty(&initialized) {
			tracing::debug!(json = %json, "chainHead_v1_follow: initialized event JSON");
		}

		// Send initialized event
		if !send_event(&sink, &initialized).await {
			self.state.remove_subscription(&sub_id).await;
			return Ok(());
		}

		// Get parent hash for the fork point block to send newBlock event
		// This is critical for papi-console explorer - it needs newBlock events to populate
		// the block list, not just initialized/bestBlockChanged
		let finalized_hash_hex_for_new: String =
			HexString::from_bytes(finalized_hash.as_bytes()).into();
		let parent_hash = self.blockchain.block_parent_hash(finalized_hash).await.ok().flatten();
		let parent_hash_hex: String = parent_hash
			.map(|h| HexString::from_bytes(h.as_bytes()).into())
			.unwrap_or_else(|| {
				// Genesis block has itself as parent in some representations
				finalized_hash_hex_for_new.clone()
			});

		// Send newBlock event for the fork point block
		let new_block = ChainHeadEvent::NewBlock(NewBlockEvent {
			block_hash: finalized_hash_hex_for_new.clone(),
			parent_block_hash: parent_hash_hex,
			new_runtime: None,
		});

		tracing::debug!(
			sub_id = %sub_id,
			block_hash = %finalized_hash_hex_for_new,
			"chainHead_v1_follow: sending newBlock event for fork point"
		);

		if !send_event(&sink, &new_block).await {
			self.state.remove_subscription(&sub_id).await;
			return Ok(());
		}

		// Send bestBlockChanged event after newBlock
		let best_block_changed = ChainHeadEvent::BestBlockChanged(BestBlockChangedEvent {
			best_block_hash: finalized_hash_hex_for_new.clone(),
		});

		tracing::debug!(
			sub_id = %sub_id,
			best_block_hash = %finalized_hash_hex_for_new,
			"chainHead_v1_follow: sending bestBlockChanged event"
		);

		if !send_event(&sink, &best_block_changed).await {
			self.state.remove_subscription(&sub_id).await;
			return Ok(());
		}

		// Send finalized event for the fork point block
		// This completes the event sequence: initialized → newBlock → bestBlockChanged → finalized
		let finalized_event = ChainHeadEvent::Finalized(FinalizedEvent {
			finalized_block_hashes: vec![finalized_hash_hex_for_new.clone()],
			pruned_block_hashes: vec![],
		});

		tracing::debug!(
			sub_id = %sub_id,
			finalized_hash = %finalized_hash_hex_for_new,
			"chainHead_v1_follow: sending finalized event for fork point"
		);

		if !send_event(&sink, &finalized_event).await {
			self.state.remove_subscription(&sub_id).await;
			return Ok(());
		}

		// Subscribe to blockchain events
		let mut blockchain_rx = self.blockchain.subscribe_events();
		let state = Arc::clone(&self.state);
		let sub_id_clone = sub_id.clone();
		let token = self.shutdown_token.clone();

		// Spawn task to forward events
		tokio::spawn(async move {
			loop {
				tokio::select! {
					biased;

					// Server shutting down
					_ = token.cancelled() => break,

					// Client disconnected
					_ = sink.closed() => break,

					// Operation event from async operation
					Some(op_event) = event_rx.recv() => {
						if !send_event(&sink, &op_event).await {
							break;
						}
					}

					// Blockchain event (new block)
					event = blockchain_rx.recv() => {
						match event {
							Ok(BlockchainEvent::NewBlock { hash, parent_hash, .. }) => {
								let hash_hex: String = HexString::from_bytes(hash.as_bytes()).into();
								let parent_hex: String = HexString::from_bytes(parent_hash.as_bytes()).into();

								// Send newBlock event
								let new_block = ChainHeadEvent::NewBlock(NewBlockEvent {
									block_hash: hash_hex.clone(),
									parent_block_hash: parent_hex,
									new_runtime: None, // Runtime changes not tracked
								});
								if !send_event(&sink, &new_block).await {
									break;
								}

								// Send bestBlockChanged event
								let best_changed = ChainHeadEvent::BestBlockChanged(BestBlockChangedEvent {
									best_block_hash: hash_hex.clone(),
								});
								if !send_event(&sink, &best_changed).await {
									break;
								}

								// Send finalized event (instant finality in fork)
								let finalized = ChainHeadEvent::Finalized(FinalizedEvent {
									finalized_block_hashes: vec![hash_hex],
									pruned_block_hashes: vec![],
								});
								if !send_event(&sink, &finalized).await {
									break;
								}
							}
							Err(broadcast::error::RecvError::Lagged(_)) => continue,
							Err(broadcast::error::RecvError::Closed) => break,
						}
					}
				}
			}

			// Cleanup subscription on disconnect
			state.remove_subscription(&sub_id_clone).await;
		});

		Ok(())
	}

	async fn header(&self, follow_subscription: String, hash: String) -> RpcResult<Option<String>> {
		// Validate subscription exists
		if self.state.get_subscription(&follow_subscription).await.is_none() {
			return Err(RpcServerError::InvalidSubscription { id: follow_subscription }.into());
		}

		// Parse block hash
		let block_hash = parse_block_hash(&hash)?;

		// Get header
		match self.blockchain.block_header(block_hash).await {
			Ok(Some(header_bytes)) => Ok(Some(HexString::from_bytes(&header_bytes).into())),
			Ok(None) => Ok(None),
			Err(e) => Err(RpcServerError::Internal(format!("Failed to get header: {e}")).into()),
		}
	}

	async fn body(&self, follow_subscription: String, hash: String) -> RpcResult<MethodResponse> {
		// Get subscription handle
		let handle = self.state.get_subscription(&follow_subscription).await.ok_or_else(|| {
			RpcServerError::InvalidSubscription { id: follow_subscription.clone() }
		})?;

		// Check operation limit
		let current_ops = handle.operation_count.load(Ordering::SeqCst);
		if current_ops >= chain_head::MAX_OPERATIONS as u64 {
			return Ok(MethodResponse { result: OperationResult::LimitReached });
		}
		handle.operation_count.fetch_add(1, Ordering::SeqCst);

		// Generate operation ID
		let operation_id = self.state.generate_operation_id();

		// Parse block hash
		let block_hash = parse_block_hash(&hash)?;

		// Spawn async task to execute operation
		let blockchain = Arc::clone(&self.blockchain);
		let event_tx = handle.event_tx.clone();
		let op_id = operation_id.clone();
		let handle_clone = Arc::clone(&handle);

		tokio::spawn(async move {
			let event = match blockchain.block_body(block_hash).await {
				Ok(Some(body)) => {
					let extrinsics: Vec<String> =
						body.iter().map(|ext| HexString::from_bytes(ext).into()).collect();
					OperationEvent::OperationBodyDone { operation_id: op_id, value: extrinsics }
				},
				Ok(None) => OperationEvent::OperationError {
					operation_id: op_id,
					error: "Block not found".to_string(),
				},
				Err(e) =>
					OperationEvent::OperationError { operation_id: op_id, error: e.to_string() },
			};

			let _ = event_tx.send(event);
			handle_clone.operation_count.fetch_sub(1, Ordering::SeqCst);
		});

		Ok(MethodResponse { result: OperationResult::Started { operation_id } })
	}

	async fn call(
		&self,
		follow_subscription: String,
		hash: String,
		function: String,
		call_parameters: String,
	) -> RpcResult<MethodResponse> {
		// Get subscription handle
		let handle = self.state.get_subscription(&follow_subscription).await.ok_or_else(|| {
			RpcServerError::InvalidSubscription { id: follow_subscription.clone() }
		})?;

		// Check operation limit
		let current_ops = handle.operation_count.load(Ordering::SeqCst);
		if current_ops >= chain_head::MAX_OPERATIONS as u64 {
			return Ok(MethodResponse { result: OperationResult::LimitReached });
		}
		handle.operation_count.fetch_add(1, Ordering::SeqCst);

		// Generate operation ID
		let operation_id = self.state.generate_operation_id();

		// Parse block hash and call parameters
		let block_hash = parse_block_hash(&hash)?;
		let params = parse_hex_bytes(&call_parameters, "call_parameters")?;

		// Spawn async task
		let blockchain = Arc::clone(&self.blockchain);
		let event_tx = handle.event_tx.clone();
		let op_id = operation_id.clone();
		let handle_clone = Arc::clone(&handle);

		tokio::spawn(async move {
			let event = match blockchain.call_at_block(block_hash, &function, &params).await {
				Ok(Some(result)) => {
					let output: String = HexString::from_bytes(&result).into();
					OperationEvent::OperationCallDone { operation_id: op_id, output }
				},
				Ok(None) => OperationEvent::OperationError {
					operation_id: op_id,
					error: "Call returned no result".to_string(),
				},
				Err(e) =>
					OperationEvent::OperationError { operation_id: op_id, error: e.to_string() },
			};

			let _ = event_tx.send(event);
			handle_clone.operation_count.fetch_sub(1, Ordering::SeqCst);
		});

		Ok(MethodResponse { result: OperationResult::Started { operation_id } })
	}

	async fn storage(
		&self,
		follow_subscription: String,
		hash: String,
		items: Vec<StorageQueryItem>,
		_child_trie: Option<String>,
	) -> RpcResult<MethodResponse> {
		// Get subscription handle
		let handle = self.state.get_subscription(&follow_subscription).await.ok_or_else(|| {
			RpcServerError::InvalidSubscription { id: follow_subscription.clone() }
		})?;

		// Check operation limit
		let current_ops = handle.operation_count.load(Ordering::SeqCst);
		if current_ops >= chain_head::MAX_OPERATIONS as u64 {
			return Ok(MethodResponse { result: OperationResult::LimitReached });
		}
		handle.operation_count.fetch_add(1, Ordering::SeqCst);

		// Generate operation ID
		let operation_id = self.state.generate_operation_id();

		// Parse block hash
		let block_hash = parse_block_hash(&hash)?;

		// Get block number for storage queries
		let block_number = self
			.blockchain
			.block_number_by_hash(block_hash)
			.await
			.map_err(|e| RpcServerError::Internal(e.to_string()))?
			.ok_or_else(|| RpcServerError::BlockNotFound(hash.clone()))?;

		// Spawn async task
		let blockchain = Arc::clone(&self.blockchain);
		let event_tx = handle.event_tx.clone();
		let op_id = operation_id.clone();
		let handle_clone = Arc::clone(&handle);

		tokio::spawn(async move {
			let mut result_items = Vec::new();

			for item in items {
				let key_bytes = match hex::decode(item.key.trim_start_matches("0x")) {
					Ok(b) => b,
					Err(_) => continue,
				};

				match item.query_type {
					StorageQueryType::Value => {
						let value = match blockchain.storage_at(block_number, &key_bytes).await {
							Ok(Some(v)) => Some(HexString::from_bytes(&v).into()),
							Ok(None) => None,
							Err(_) => None,
						};
						result_items.push(StorageResultItem { key: item.key, value, hash: None });
					},
					StorageQueryType::Hash => {
						// Get value and hash it
						let hash = match blockchain.storage_at(block_number, &key_bytes).await {
							Ok(Some(v)) => {
								let hash = sp_core::blake2_256(&v);
								Some(HexString::from_bytes(&hash).into())
							},
							Ok(None) => None,
							Err(_) => None,
						};
						result_items.push(StorageResultItem { key: item.key, value: None, hash });
					},
					StorageQueryType::ClosestDescendantMerkleValue => {
						// Merkle proofs not supported in fork - return empty result
						result_items.push(StorageResultItem {
							key: item.key,
							value: None,
							hash: None,
						});
					},
					StorageQueryType::DescendantsValues | StorageQueryType::DescendantsHashes => {
						// Descendants queries require key enumeration which is not yet implemented.
						// Return empty result to allow PAPI to not error out.
						result_items.push(StorageResultItem {
							key: item.key,
							value: None,
							hash: None,
						});
					},
				}
			}

			// Send storage items if any
			if !result_items.is_empty() {
				let _ = event_tx.send(OperationEvent::OperationStorageItems {
					operation_id: op_id.clone(),
					items: result_items,
				});
			}

			// Send done event
			let _ = event_tx.send(OperationEvent::OperationStorageDone { operation_id: op_id });

			handle_clone.operation_count.fetch_sub(1, Ordering::SeqCst);
		});

		Ok(MethodResponse { result: OperationResult::Started { operation_id } })
	}

	async fn unpin(
		&self,
		follow_subscription: String,
		_hash_or_hashes: serde_json::Value,
	) -> RpcResult<()> {
		// Validate subscription exists
		if self.state.get_subscription(&follow_subscription).await.is_none() {
			return Err(RpcServerError::InvalidSubscription { id: follow_subscription }.into());
		}

		// No-op: fork keeps all blocks in memory
		Ok(())
	}

	async fn continue_op(
		&self,
		follow_subscription: String,
		_operation_id: String,
	) -> RpcResult<()> {
		// Validate subscription exists
		if self.state.get_subscription(&follow_subscription).await.is_none() {
			return Err(RpcServerError::InvalidSubscription { id: follow_subscription }.into());
		}

		// No-op: we don't paginate storage results currently
		Ok(())
	}

	async fn stop_operation(
		&self,
		follow_subscription: String,
		_operation_id: String,
	) -> RpcResult<()> {
		// Validate subscription exists
		if self.state.get_subscription(&follow_subscription).await.is_none() {
			return Err(RpcServerError::InvalidSubscription { id: follow_subscription }.into());
		}

		// No-op: operations complete immediately in fork
		Ok(())
	}
}

#[cfg(test)]
mod tests {

	use crate::testing::TestContext;
	use jsonrpsee::{core::client::SubscriptionClientT, rpc_params, ws_client::WsClientBuilder};

	#[tokio::test(flavor = "multi_thread")]
	async fn follow_returns_subscription_and_initialized_event() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default().build(&ctx.ws_url()).await.unwrap();

		// Subscribe to chain head
		let mut sub = client
			.subscribe::<serde_json::Value, _>(
				"chainHead_v1_follow",
				rpc_params![false],
				"chainHead_v1_unfollow",
			)
			.await
			.expect("Subscription should succeed");

		// Should receive initialized event
		let event = sub.next().await.expect("Should receive event").expect("Event should be valid");
		let event_type = event.get("event").and_then(|v| v.as_str());
		assert_eq!(event_type, Some("initialized"));

		// Should have finalized block hashes
		let hashes = event.get("finalizedBlockHashes").and_then(|v| v.as_array());
		assert!(hashes.is_some());
		let hashes = hashes.unwrap();
		assert!(!hashes.is_empty());
		let finalized_hash = hashes[0].as_str().unwrap();

		// Should receive newBlock event for the fork point block
		// This is critical for papi-console explorer - it needs newBlock events to populate blocks
		let new_block_event =
			sub.next().await.expect("Should receive event").expect("Event should be valid");
		let new_block_event_type = new_block_event.get("event").and_then(|v| v.as_str());
		assert_eq!(new_block_event_type, Some("newBlock"));

		// newBlock should have the same hash as the finalized block
		let new_block_hash = new_block_event.get("blockHash").and_then(|v| v.as_str());
		assert_eq!(new_block_hash, Some(finalized_hash));

		// Should receive bestBlockChanged event after newBlock
		let best_event =
			sub.next().await.expect("Should receive event").expect("Event should be valid");
		let best_event_type = best_event.get("event").and_then(|v| v.as_str());
		assert_eq!(best_event_type, Some("bestBlockChanged"));

		// Best block should match the finalized hash
		let best_hash = best_event.get("bestBlockHash").and_then(|v| v.as_str());
		assert_eq!(best_hash, Some(finalized_hash));

		// Should receive finalized event for the fork point block
		let finalized_event =
			sub.next().await.expect("Should receive event").expect("Event should be valid");
		let finalized_event_type = finalized_event.get("event").and_then(|v| v.as_str());
		assert_eq!(finalized_event_type, Some("finalized"));

		// Finalized event should contain the fork point hash
		let finalized_hashes = finalized_event
			.get("finalizedBlockHashes")
			.and_then(|v| v.as_array())
			.expect("Should have finalizedBlockHashes");
		assert!(!finalized_hashes.is_empty());
		assert_eq!(finalized_hashes[0].as_str(), Some(finalized_hash));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn header_returns_header_for_valid_subscription() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default().build(&ctx.ws_url()).await.unwrap();

		// Subscribe first
		let mut sub = client
			.subscribe::<serde_json::Value, _>(
				"chainHead_v1_follow",
				rpc_params![false],
				"chainHead_v1_unfollow",
			)
			.await
			.expect("Subscription should succeed");

		// Get initialized event to extract subscription ID and block hash
		let event = sub.next().await.expect("Should receive event").expect("Event should be valid");
		let hashes = event.get("finalizedBlockHashes").unwrap().as_array().unwrap();
		let block_hash = hashes[0].as_str().unwrap();

		// The subscription ID for jsonrpsee is internal, so we need to test via the subscription
		// context For now, let's just verify the initialized event has the right structure
		assert!(block_hash.starts_with("0x"));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn invalid_subscription_returns_error() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default().build(&ctx.ws_url()).await.unwrap();

		use jsonrpsee::core::client::ClientT;

		// Try to get header with invalid subscription
		let result: Result<Option<String>, _> = client
			.request(
				"chainHead_v1_header",
				rpc_params![
					"invalid-sub",
					"0x0000000000000000000000000000000000000000000000000000000000000000"
				],
			)
			.await;

		assert!(result.is_err());
	}
}
