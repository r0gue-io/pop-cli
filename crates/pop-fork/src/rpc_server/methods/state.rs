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
use scale::Decode;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

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
pub struct StateApi {
	blockchain: Arc<Blockchain>,
	shutdown_token: CancellationToken,
}

impl StateApi {
	/// Create a new StateApi instance.
	pub fn new(blockchain: Arc<Blockchain>, shutdown_token: CancellationToken) -> Self {
		Self { blockchain, shutdown_token }
	}
}

#[async_trait::async_trait]
impl StateApiServer for StateApi {
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
			None => self.blockchain.head().await.hash,
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
		let block_hash = match at {
			Some(hash) => parse_block_hash(&hash)?,
			None => self.blockchain.head().await.hash,
		};
		// Fetch runtime version via Core_version call and decode
		match self.blockchain.call_at_block(block_hash, runtime_api::CORE_VERSION, &[]).await {
			Ok(Some(result)) => {
				// Decode the SCALE-encoded RuntimeVersion
				// Format: spec_name, impl_name, authoring_version, spec_version, impl_version,
				//         apis (Vec<([u8;8], u32)>), transaction_version, state_version
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
			},
			_ => Err(RpcServerError::Internal("Failed to get runtime version".to_string()).into()),
		}
	}

	async fn get_keys_paged(
		&self,
		_prefix: Option<String>,
		_count: u32,
		_start_key: Option<String>,
		_at: Option<String>,
	) -> RpcResult<Vec<String>> {
		// Mock implementation - return empty list
		// Full implementation would require iterating storage keys which is complex
		Ok(vec![])
	}

	async fn call(&self, method: String, data: String, at: Option<String>) -> RpcResult<String> {
		let block_hash = match at {
			Some(hash) => parse_block_hash(&hash)?,
			None => self.blockchain.head().await.hash,
		};
		let params = parse_hex_bytes(&data, "data")?;

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
			None => {
				let head = self.blockchain.head().await;
				(head.number, head.hash)
			},
		};

		// Query each key
		let mut changes = Vec::new();
		for key in keys {
			let key_bytes = parse_hex_bytes(&key, "key")?;
			let value = match self.blockchain.storage_at(block_number, &key_bytes).await {
				Ok(Some(v)) => Some(HexString::from_bytes(&v).into()),
				Ok(None) => None,
				Err(_) => None,
			};
			changes.push((key, value));
		}

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

					_ = sink.closed() => break,

					event = receiver.recv() => {
						match event {
							Ok(BlockchainEvent::NewBlock { hash, modified_keys, .. }) => {
								// Filter to keys we're watching that were modified
								let affected_indices: Vec<usize> = subscribed_keys.iter()
									.enumerate()
									.filter(|(_, k)| modified_keys.iter().any(|mk| mk == *k))
									.map(|(i, _)| i)
									.collect();

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
									Err(_) => continue,
								};
								if sink.send(msg).await.is_err() {
									break;
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
}

#[cfg(test)]
mod tests {

	use crate::{
		rpc_server::types::RuntimeVersion,
		testing::{TestContext, accounts::ALICE, helpers::account_storage_key},
	};
	use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClientBuilder};

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_storage_returns_value() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Query Alice's account storage (should exist on dev chain)
		let key = account_storage_key(&ALICE);
		let key_hex = format!("0x{}", hex::encode(&key));

		let result: Option<String> = client
			.request("state_getStorage", rpc_params![key_hex])
			.await
			.expect("RPC call failed");

		assert!(result.is_some(), "Alice's account should exist");
		let value = result.unwrap();
		assert!(value.starts_with("0x"), "Value should be hex encoded");
		assert!(value.len() > 2, "Value should not be empty");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_storage_at_block_hash() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		let block = ctx.blockchain().build_empty_block().await.unwrap();
		ctx.blockchain().build_empty_block().await.unwrap();

		// Get current head hash
		let block_hash_hex = format!("0x{}", hex::encode(block.hash.as_bytes()));

		// Query Alice's account storage at specific block
		let key = account_storage_key(&ALICE);
		let key_hex = format!("0x{}", hex::encode(&key));

		let result: Option<String> = client
			.request("state_getStorage", rpc_params![key_hex, block_hash_hex])
			.await
			.expect("RPC call failed");

		assert!(result.is_some(), "Alice's account should exist at block");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_storage_returns_none_for_nonexistent_key() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Query a nonexistent storage key
		let fake_key = "0x0000000000000000000000000000000000000000000000000000000000000000";

		let result: Option<String> = client
			.request("state_getStorage", rpc_params![fake_key])
			.await
			.expect("RPC call failed");

		assert!(result.is_none(), "Nonexistent key should return None");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_metadata_returns_metadata() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		let result: String = client
			.request("state_getMetadata", rpc_params![])
			.await
			.expect("RPC call failed");

		assert!(result.starts_with("0x"), "Metadata should be hex encoded");
		// Metadata is large, just check it's substantial
		assert!(result.len() > 1000, "Metadata should be substantial in size");
		// Verify metadata magic number "meta" (0x6d657461) is at the start
		assert!(
			result.starts_with("0x6d657461"),
			"Metadata should start with magic number 'meta' (0x6d657461), got: {}",
			&result[..20.min(result.len())]
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_metadata_at_block_hash() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Get current head hash
		let head_hash = ctx.blockchain().head_hash().await;
		let head_hash_hex = format!("0x{}", hex::encode(head_hash.as_bytes()));

		let result: String = client
			.request("state_getMetadata", rpc_params![head_hash_hex])
			.await
			.expect("RPC call failed");

		assert!(result.starts_with("0x"), "Metadata should be hex encoded");
		assert!(result.len() > 1000, "Metadata should be substantial in size");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_runtime_version_returns_version() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		let result: RuntimeVersion = client
			.request("state_getRuntimeVersion", rpc_params![])
			.await
			.expect("RPC call failed");

		// Verify we got a valid runtime version
		assert!(!result.spec_name.is_empty(), "Spec name should not be empty");
		assert!(!result.impl_name.is_empty(), "Impl name should not be empty");
		assert!(result.spec_version > 0, "Spec version should be positive");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_runtime_version_at_block_hash() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		// Get current head hash
		let head_hash = ctx.blockchain().head_hash().await;
		let head_hash_hex = format!("0x{}", hex::encode(head_hash.as_bytes()));

		let result: RuntimeVersion = client
			.request("state_getRuntimeVersion", rpc_params![head_hash_hex])
			.await
			.expect("RPC call failed");

		assert!(!result.spec_name.is_empty(), "Spec name should not be empty");
		assert!(result.spec_version > 0, "Spec version should be positive");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_storage_invalid_hex_returns_error() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		let result: Result<Option<String>, _> =
			client.request("state_getStorage", rpc_params!["not_valid_hex"]).await;

		assert!(result.is_err(), "Should fail with invalid hex key");
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn state_get_storage_invalid_block_hash_returns_error() {
		let ctx = TestContext::for_rpc_server().await;
		let client = WsClientBuilder::default()
			.build(&ctx.ws_url())
			.await
			.expect("Failed to connect");

		let key = account_storage_key(&ALICE);
		let key_hex = format!("0x{}", hex::encode(&key));
		let invalid_hash = "0x0000000000000000000000000000000000000000000000000000000000000000";

		let result: Result<Option<String>, _> =
			client.request("state_getStorage", rpc_params![key_hex, invalid_hash]).await;

		assert!(result.is_err(), "Should fail with invalid block hash");
	}
}
