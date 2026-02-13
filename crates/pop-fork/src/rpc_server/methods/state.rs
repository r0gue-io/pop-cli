// SPDX-License-Identifier: GPL-3.0

//! Legacy state_* RPC methods.
//!
//! These methods provide state-related operations for polkadot.js compatibility.

use crate::{
	Blockchain, BlockchainEvent,
	rpc_server::{
		RpcServerError, parse_block_hash, parse_hex_bytes,
		types::{HexString, RuntimeVersion, StorageChangeSet},
	},
	strings::rpc_server::{runtime_api, storage},
};
use jsonrpsee::{
	PendingSubscriptionSink,
	core::{RpcResult, SubscriptionResult},
	proc_macros::rpc,
};
use log::{debug, warn};
use scale::Decode;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

#[async_trait::async_trait]
pub trait StateBlockchain: Send + Sync {
	async fn head_snapshot(&self) -> (u32, subxt::utils::H256);
	async fn head_number(&self) -> u32;
	async fn head_hash(&self) -> subxt::utils::H256;
	async fn block_number_by_hash(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<u32>, crate::BlockchainError>;
	async fn storage_at(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError>;
	async fn call_at_block(
		&self,
		hash: subxt::utils::H256,
		method: &str,
		params: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError>;
	fn fork_point_number(&self) -> u32;
	async fn proxy_state_call(
		&self,
		method: &str,
		params: &[u8],
		at_hash: subxt::utils::H256,
	) -> Result<Vec<u8>, crate::BlockchainError>;
	async fn storage_keys_paged(
		&self,
		prefix: &[u8],
		count: u32,
		start_key: Option<&[u8]>,
		at: Option<subxt::utils::H256>,
	) -> Result<Vec<Vec<u8>>, crate::BlockchainError>;
	async fn storage_batch(
		&self,
		block_hash: subxt::utils::H256,
		keys: &[&[u8]],
	) -> Result<Vec<Option<Vec<u8>>>, crate::BlockchainError>;
	async fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, crate::BlockchainError>;
	fn subscribe_events(&self) -> broadcast::Receiver<BlockchainEvent>;
}

#[async_trait::async_trait]
impl StateBlockchain for Blockchain {
	async fn head_snapshot(&self) -> (u32, subxt::utils::H256) {
		let head = Blockchain::head(self).await;
		(head.number, head.hash)
	}

	async fn head_number(&self) -> u32 {
		Blockchain::head_number(self).await
	}

	async fn head_hash(&self) -> subxt::utils::H256 {
		Blockchain::head_hash(self).await
	}

	async fn block_number_by_hash(
		&self,
		hash: subxt::utils::H256,
	) -> Result<Option<u32>, crate::BlockchainError> {
		Blockchain::block_number_by_hash(self, hash).await
	}

	async fn storage_at(
		&self,
		block_number: u32,
		key: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
		Blockchain::storage_at(self, block_number, key).await
	}

	async fn call_at_block(
		&self,
		hash: subxt::utils::H256,
		method: &str,
		params: &[u8],
	) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
		Blockchain::call_at_block(self, hash, method, params).await
	}

	fn fork_point_number(&self) -> u32 {
		Blockchain::fork_point_number(self)
	}

	async fn proxy_state_call(
		&self,
		method: &str,
		params: &[u8],
		at_hash: subxt::utils::H256,
	) -> Result<Vec<u8>, crate::BlockchainError> {
		Blockchain::proxy_state_call(self, method, params, at_hash).await
	}

	async fn storage_keys_paged(
		&self,
		prefix: &[u8],
		count: u32,
		start_key: Option<&[u8]>,
		at: Option<subxt::utils::H256>,
	) -> Result<Vec<Vec<u8>>, crate::BlockchainError> {
		Blockchain::storage_keys_paged(self, prefix, count, start_key, at).await
	}

	async fn storage_batch(
		&self,
		block_hash: subxt::utils::H256,
		keys: &[&[u8]],
	) -> Result<Vec<Option<Vec<u8>>>, crate::BlockchainError> {
		Blockchain::storage_batch(self, block_hash, keys).await
	}

	async fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, crate::BlockchainError> {
		Blockchain::storage(self, key).await
	}

	fn subscribe_events(&self) -> broadcast::Receiver<BlockchainEvent> {
		Blockchain::subscribe_events(self)
	}
}

/// Legacy state RPC methods.
#[rpc(server, namespace = "state")]
pub trait StateApi {
	/// Get storage value at a key.
	///
	/// Returns the hex-encoded storage value at the given key, or null if no value exists.
	#[method(name = "getStorage")]
	async fn get_storage(&self, key: String, at: Option<String>) -> RpcResult<Option<String>>;

	/// Get the runtime metadata.
	///
	/// Returns the hex-encoded runtime metadata.
	#[method(name = "getMetadata")]
	async fn get_metadata(&self, at: Option<String>) -> RpcResult<String>;

	/// Get the runtime version.
	#[method(name = "getRuntimeVersion")]
	async fn get_runtime_version(&self, at: Option<String>) -> RpcResult<RuntimeVersion>;

	/// Get storage keys with pagination.
	#[method(name = "getKeysPaged")]
	async fn get_keys_paged(
		&self,
		prefix: Option<String>,
		count: u32,
		start_key: Option<String>,
		at: Option<String>,
	) -> RpcResult<Vec<String>>;

	/// Call a runtime API method.
	///
	/// Returns the hex-encoded result of the runtime call.
	#[method(name = "call")]
	async fn call(&self, method: String, data: String, at: Option<String>) -> RpcResult<String>;

	/// Subscribe to runtime version changes.
	#[subscription(name = "subscribeRuntimeVersion" => "runtimeVersion", unsubscribe = "unsubscribeRuntimeVersion", item = RuntimeVersion)]
	async fn subscribe_runtime_version(&self) -> SubscriptionResult;

	/// Subscribe to storage changes.
	#[subscription(name = "subscribeStorage" => "storage", unsubscribe = "unsubscribeStorage", item = StorageChangeSet)]
	async fn subscribe_storage(&self, keys: Option<Vec<String>>) -> SubscriptionResult;

	/// Query storage values at a specific block.
	///
	/// Returns a list of storage change sets with the values for each key.
	#[method(name = "queryStorageAt")]
	async fn query_storage_at(
		&self,
		keys: Vec<String>,
		at: Option<String>,
	) -> RpcResult<Vec<StorageChangeSet>>;
}

/// Implementation of legacy state RPC methods.
pub struct StateApi<T: StateBlockchain = Blockchain> {
	blockchain: Arc<T>,
	shutdown_token: CancellationToken,
}

impl<T: StateBlockchain> StateApi<T> {
	/// Create a new StateApi instance.
	pub fn new(blockchain: Arc<T>, shutdown_token: CancellationToken) -> Self {
		Self { blockchain, shutdown_token }
	}
}

#[async_trait::async_trait]
impl<T: StateBlockchain + 'static> StateApiServer for StateApi<T> {
	async fn get_storage(&self, key: String, at: Option<String>) -> RpcResult<Option<String>> {
		let block_number = match at {
			Some(hash) => {
				let block_hash = parse_block_hash(&hash)?;
				self.blockchain
					.block_number_by_hash(block_hash)
					.await
					.map_err(|_| {
						RpcServerError::InvalidParam(format!("Invalid block hash: {}", hash))
					})?
					.ok_or_else(|| {
						RpcServerError::InvalidParam(format!("Invalid block hash: {}", hash))
					})?
			},
			None => self.blockchain.head_number().await,
		};
		let key_bytes = parse_hex_bytes(&key, "key")?;

		// Query storage from blockchain
		match self.blockchain.storage_at(block_number, &key_bytes).await {
			Ok(Some(value)) => Ok(Some(HexString::from_bytes(&value).into())),
			Ok(None) => Ok(None),
			Err(e) => Err(RpcServerError::Storage(e.to_string()).into()),
		}
	}

	async fn get_metadata(&self, at: Option<String>) -> RpcResult<String> {
		let block_hash = match at {
			Some(hash) => parse_block_hash(&hash)?,
			None => self.blockchain.head_hash().await,
		};
		// Fetch real metadata via runtime call
		// The runtime returns SCALE-encoded Vec<u8>, so we need to decode it
		// to strip the compact length prefix
		match self.blockchain.call_at_block(block_hash, runtime_api::METADATA, &[]).await {
			Ok(Some(result)) => {
				// Decode SCALE Vec<u8> to get raw metadata bytes
				let metadata_bytes: Vec<u8> = Decode::decode(&mut &result[..]).map_err(|e| {
					RpcServerError::Internal(format!("Failed to decode metadata: {}", e))
				})?;
				Ok(HexString::from_bytes(&metadata_bytes).into())
			},
			_ => Err(RpcServerError::Internal("Failed to get metadata".to_string()).into()),
		}
	}

	async fn get_runtime_version(&self, at: Option<String>) -> RpcResult<RuntimeVersion> {
		let (block_number, block_hash) = match at {
			Some(ref hash) => {
				let parsed = parse_block_hash(hash)?;
				let num = self
					.blockchain
					.block_number_by_hash(parsed)
					.await
					.map_err(|_| {
						RpcServerError::InvalidParam(format!("Invalid block hash: {hash}"))
					})?
					.ok_or_else(|| {
						RpcServerError::InvalidParam(format!("Block not found: {hash}"))
					})?;
				(num, parsed)
			},
			None => self.blockchain.head_snapshot().await,
		};

		// For blocks at or before the fork point, proxy Core_version to the upstream
		// node. Its JIT-compiled runtime is orders of magnitude faster than the local
		// WASM interpreter.
		let result = if block_number <= self.blockchain.fork_point_number() {
			self.blockchain
				.proxy_state_call(runtime_api::CORE_VERSION, &[], block_hash)
				.await
				.ok()
		} else {
			None
		};

		// Fall back to local WASM execution for fork-local blocks or proxy failure.
		let result = match result {
			Some(r) => r,
			None => self
				.blockchain
				.call_at_block(block_hash, runtime_api::CORE_VERSION, &[])
				.await
				.map_err(|e| {
					RpcServerError::Internal(format!("Failed to get runtime version: {e}"))
				})?
				.ok_or_else(|| {
					RpcServerError::Internal("Failed to get runtime version".to_string())
				})?,
		};

		// Decode the SCALE-encoded RuntimeVersion.
		#[derive(Decode)]
		struct ScaleRuntimeVersion {
			spec_name: String,
			impl_name: String,
			authoring_version: u32,
			spec_version: u32,
			impl_version: u32,
			apis: Vec<([u8; 8], u32)>,
			transaction_version: u32,
			state_version: u8,
		}
		let version = ScaleRuntimeVersion::decode(&mut result.as_slice()).map_err(|e| {
			RpcServerError::Internal(format!("Failed to decode runtime version: {e}"))
		})?;

		Ok(RuntimeVersion {
			spec_name: version.spec_name,
			impl_name: version.impl_name,
			authoring_version: version.authoring_version,
			spec_version: version.spec_version,
			impl_version: version.impl_version,
			transaction_version: version.transaction_version,
			state_version: version.state_version,
			apis: version
				.apis
				.into_iter()
				.map(|(id, ver)| (HexString::from_bytes(&id).into(), ver))
				.collect(),
		})
	}

	async fn get_keys_paged(
		&self,
		prefix: Option<String>,
		count: u32,
		start_key: Option<String>,
		at: Option<String>,
	) -> RpcResult<Vec<String>> {
		let prefix_bytes = match prefix {
			Some(ref p) => parse_hex_bytes(p, "prefix")?,
			None => vec![],
		};
		let start_key_bytes = match start_key {
			Some(ref k) => Some(parse_hex_bytes(k, "start_key")?),
			None => None,
		};
		let block_hash = match at {
			Some(ref hash) => Some(parse_block_hash(hash)?),
			None => None,
		};

		jsonrpsee::tracing::debug!(
			prefix = ?prefix,
			count = count,
			start_key = ?start_key,
			at = ?at,
			"state_getKeysPaged: querying storage keys"
		);

		let keys = self
			.blockchain
			.storage_keys_paged(&prefix_bytes, count, start_key_bytes.as_deref(), block_hash)
			.await
			.map_err(|e| RpcServerError::Storage(e.to_string()))?;

		jsonrpsee::tracing::debug!(
			keys_returned = keys.len(),
			"state_getKeysPaged: returning keys"
		);

		Ok(keys.into_iter().map(|k| HexString::from_bytes(&k).into()).collect())
	}

	async fn call(&self, method: String, data: String, at: Option<String>) -> RpcResult<String> {
		let params = parse_hex_bytes(&data, "data")?;

		let (block_number, block_hash) = match at {
			Some(ref hash) => {
				let parsed = parse_block_hash(hash)?;
				let num = self
					.blockchain
					.block_number_by_hash(parsed)
					.await
					.map_err(|_| {
						RpcServerError::InvalidParam(format!("Invalid block hash: {hash}"))
					})?
					.ok_or_else(|| {
						RpcServerError::InvalidParam(format!("Block not found: {hash}"))
					})?;
				(num, parsed)
			},
			None => self.blockchain.head_snapshot().await,
		};

		// Proxy metadata runtime API calls to the upstream RPC for performance,
		// but only for blocks at or before the fork point where the runtime is
		// guaranteed to match the upstream. Fork-local blocks may have a different
		// runtime due to upgrades.
		if method.starts_with(runtime_api::METADATA_PREFIX) &&
			block_number <= self.blockchain.fork_point_number()
		{
			match self.blockchain.proxy_state_call(&method, &params, block_hash).await {
				Ok(result) => return Ok(HexString::from_bytes(&result).into()),
				Err(e) => {
					jsonrpsee::tracing::debug!(
						"Upstream proxy failed for {method}, falling back to local execution: {e}"
					);
				},
			}
		}

		match self.blockchain.call_at_block(block_hash, &method, &params).await {
			Ok(Some(result)) => Ok(HexString::from_bytes(&result).into()),
			Ok(None) => Err(RpcServerError::Internal("Call returned no result".to_string()).into()),
			Err(e) => Err(RpcServerError::Internal(format!("Runtime call failed: {}", e)).into()),
		}
	}

	async fn query_storage_at(
		&self,
		keys: Vec<String>,
		at: Option<String>,
	) -> RpcResult<Vec<StorageChangeSet>> {
		// Resolve block
		let (block_number, block_hash) = match at {
			Some(ref hash) => {
				let parsed = parse_block_hash(hash)?;
				let num = self
					.blockchain
					.block_number_by_hash(parsed)
					.await
					.map_err(|_| {
						RpcServerError::InvalidParam(format!("Invalid block hash: {}", hash))
					})?
					.ok_or_else(|| {
						RpcServerError::InvalidParam(format!("Block not found: {}", hash))
					})?;
				(num, parsed)
			},
			None => self.blockchain.head_snapshot().await,
		};

		jsonrpsee::tracing::debug!(
			num_keys = keys.len(),
			block_number = block_number,
			"state_queryStorageAt: fetching {} keys in parallel",
			keys.len()
		);

		// Parse all keys upfront
		let parsed_keys: Vec<(String, Vec<u8>)> = keys
			.into_iter()
			.map(|key| {
				let bytes = parse_hex_bytes(&key, "key")?;
				Ok((key, bytes))
			})
			.collect::<Result<Vec<_>, jsonrpsee::types::ErrorObjectOwned>>()?;

		// For blocks at or before the fork point, batch-fetch from the upstream in a
		// single RPC call. This is orders of magnitude faster than per-key fetching.
		let changes: Vec<_> = if block_number <= self.blockchain.fork_point_number() {
			let key_refs: Vec<&[u8]> = parsed_keys.iter().map(|(_, k)| k.as_slice()).collect();
			let values = self
				.blockchain
				.storage_batch(block_hash, &key_refs)
				.await
				.map_err(|e| RpcServerError::Storage(e.to_string()))?;

			parsed_keys
				.into_iter()
				.zip(values)
				.map(|((key, _), value)| (key, value.map(|v| HexString::from_bytes(&v).into())))
				.collect()
		} else {
			// For fork-local blocks, query each key through the local storage layer
			let futures: Vec<_> = parsed_keys
				.iter()
				.map(|(_, key_bytes)| self.blockchain.storage_at(block_number, key_bytes))
				.collect();
			let results = futures::future::join_all(futures).await;

			parsed_keys
				.into_iter()
				.zip(results)
				.map(|((key, _), result)| {
					let value = match result {
						Ok(Some(v)) => Some(HexString::from_bytes(&v).into()),
						_ => None,
					};
					(key, value)
				})
				.collect()
		};

		// Return as single change set for the queried block
		Ok(vec![StorageChangeSet {
			block: HexString::from_bytes(block_hash.as_bytes()).into(),
			changes,
		}])
	}

	async fn subscribe_runtime_version(
		&self,
		pending: PendingSubscriptionSink,
	) -> SubscriptionResult {
		let sink = pending.accept().await?;
		let blockchain = Arc::clone(&self.blockchain);
		let token = self.shutdown_token.clone();

		// Get current runtime version and send it immediately
		let current_version = self.get_runtime_version(None).await.ok();
		if let Some(ref version) = current_version {
			let _ = sink.send(jsonrpsee::SubscriptionMessage::from_json(version)?).await;
		}

		// Subscribe to blockchain events to detect runtime upgrades
		let mut receiver = blockchain.subscribe_events();

		// The `:code` storage key indicates a runtime upgrade
		let code_key = storage::RUNTIME_CODE_KEY.to_vec();

		tokio::spawn(async move {
			let mut last_spec_version = current_version.map(|v| v.spec_version);

			loop {
				tokio::select! {
					biased;

					_ = token.cancelled() => break,

					_ = sink.closed() => break,

					event = receiver.recv() => {
						match event {
							Ok(BlockchainEvent::NewBlock { modified_keys, hash, .. }) => {
								// Check if :code was modified (runtime upgrade)
								let runtime_changed = modified_keys.contains(&code_key);

								if runtime_changed {
									// Fetch new runtime version
									if let Ok(Some(result)) = blockchain.call_at_block(hash, runtime_api::CORE_VERSION, &[]).await {
										#[derive(Decode)]
										struct ScaleRuntimeVersion {
											spec_name: String,
											impl_name: String,
											authoring_version: u32,
											spec_version: u32,
											impl_version: u32,
											apis: Vec<([u8; 8], u32)>,
											transaction_version: u32,
											state_version: u8,
										}

										if let Ok(version) = ScaleRuntimeVersion::decode(&mut result.as_slice()) {
											// Only notify if spec_version changed
											if last_spec_version != Some(version.spec_version) {
												last_spec_version = Some(version.spec_version);

												let rt_version = RuntimeVersion {
													spec_name: version.spec_name,
													impl_name: version.impl_name,
													authoring_version: version.authoring_version,
													spec_version: version.spec_version,
													impl_version: version.impl_version,
													transaction_version: version.transaction_version,
													state_version: version.state_version,
													apis: version.apis.into_iter()
														.map(|(id, ver)| (format!("0x{}", hex::encode(id)), ver))
														.collect(),
												};

												let msg = match jsonrpsee::SubscriptionMessage::from_json(&rt_version) {
													Ok(m) => m,
													Err(_) => continue,
												};
												if sink.send(msg).await.is_err() {
													break;
												}
											}
										}
									}
								}
							}
							Err(broadcast::error::RecvError::Lagged(_)) => continue,
							Err(broadcast::error::RecvError::Closed) => break,
						}
					}
				}
			}
		});

		Ok(())
	}

	async fn subscribe_storage(
		&self,
		pending: PendingSubscriptionSink,
		keys: Option<Vec<String>>,
	) -> SubscriptionResult {
		let sink = pending.accept().await?;
		let blockchain = Arc::clone(&self.blockchain);

		// Parse subscribed keys from hex to bytes
		let subscribed_keys: Vec<Vec<u8>> = keys
			.clone()
			.unwrap_or_default()
			.iter()
			.filter_map(|k| hex::decode(k.trim_start_matches("0x")).ok())
			.collect();

		// Also keep original hex keys for response formatting
		let subscribed_keys_hex: Vec<String> = keys.unwrap_or_default();

		debug!("[state] Storage subscription accepted for {} keys", subscribed_keys.len());

		// Send initial values
		let head_hash = blockchain.head_hash().await;
		let block_hex = format!("0x{}", hex::encode(head_hash.as_bytes()));

		let mut changes = Vec::new();
		for (i, key_bytes) in subscribed_keys.iter().enumerate() {
			let value = blockchain
				.storage(key_bytes)
				.await
				.ok()
				.flatten()
				.map(|v| format!("0x{}", hex::encode(v)));
			let key_hex = subscribed_keys_hex.get(i).cloned().unwrap_or_default();
			changes.push((key_hex, value));
		}

		let change_set = StorageChangeSet { block: block_hex, changes };
		let _ = sink.send(jsonrpsee::SubscriptionMessage::from_json(&change_set)?).await;

		// If no keys to watch, just wait for close
		if subscribed_keys.is_empty() {
			sink.closed().await;
			return Ok(());
		}

		// Subscribe to blockchain events
		let mut receiver = blockchain.subscribe_events();
		let token = self.shutdown_token.clone();

		tokio::spawn(async move {
			loop {
				tokio::select! {
					biased;

					_ = token.cancelled() => break,

					_ = sink.closed() => {
						debug!("[state] Storage subscriber disconnected");
						break;
					},

					event = receiver.recv() => {
						match event {
							Ok(BlockchainEvent::NewBlock { number, hash, modified_keys, .. }) => {
								// Filter to keys we're watching that were modified
								let affected_indices: Vec<usize> = subscribed_keys.iter()
									.enumerate()
									.filter(|(_, k)| modified_keys.iter().any(|mk| mk == *k))
									.map(|(i, _)| i)
									.collect();

								debug!(
									"[state] Block #{number}: {}/{} subscribed keys affected ({} modified total)",
									affected_indices.len(),
									subscribed_keys.len(),
									modified_keys.len()
								);

								if affected_indices.is_empty() {
									continue;
								}

								// Fetch new values for affected keys
								let block_hex = format!("0x{}", hex::encode(hash.as_bytes()));
								let mut changes = Vec::new();

								for i in affected_indices {
									let key_hex = subscribed_keys_hex.get(i).cloned().unwrap_or_default();
									let key_bytes = &subscribed_keys[i];
									let value = blockchain.storage(key_bytes).await.ok().flatten()
										.map(|v| format!("0x{}", hex::encode(v)));
									changes.push((key_hex, value));
								}

								let change_set = StorageChangeSet { block: block_hex, changes };
								let msg = match jsonrpsee::SubscriptionMessage::from_json(&change_set) {
									Ok(m) => m,
									Err(e) => {
										warn!("[state] Failed to serialize storage change for #{number}: {e}");
										continue;
									},
								};
								if sink.send(msg).await.is_err() {
									debug!("[state] Storage subscriber disconnected during send");
									break;
								}
							}
							Err(broadcast::error::RecvError::Lagged(n)) => {
								warn!("[state] Storage subscriber lagged, skipped {n} events");
								continue;
							}
							Err(broadcast::error::RecvError::Closed) => {
								debug!("[state] Broadcast channel closed");
								break;
							}
						}
					}
				}
			}
		});

		Ok(())
	}
}
