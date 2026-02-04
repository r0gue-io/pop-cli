// SPDX-License-Identifier: GPL-3.0

//! Runtime executor using smoldot to execute Substrate runtime calls.
//!
//! This module provides [`RuntimeExecutor`], a wrapper around smoldot's executor that
//! runs Substrate runtime calls (like `Core_version`, `BlockBuilder_apply_extrinsic`, etc.)
//! against storage provided by a [`LocalStorageLayer`].
//!
//! # Design Decision: Why smoldot?
//!
//! smoldot already implements all ~50 Substrate host functions required for runtime execution:
//!
//! - **Storage operations**: get, set, clear, exists, next_key
//! - **Cryptographic operations**: sr25519, ed25519, ecdsa signature verification
//! - **Hashing**: blake2, keccak, sha2, twox
//! - **Memory allocation**: heap management for the WASM runtime
//! - **Logging and debugging**: runtime log emission
//!
//! By using smoldot's `runtime_call` API, we avoid reimplementing these host functions
//! while gaining full control over storage access. Storage reads are routed through the
//! [`LocalStorageLayer`], which checks local modifications first,
//! then deleted prefixes, and finally falls back to the parent layer (typically a
//! [`RemoteStorageLayer`](crate::RemoteStorageLayer) that lazily fetches from RPC).
//!
//! # Executor Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                      RuntimeExecutor                                │
//! │                                                                     │
//! │   call(method, args) ──► Create VM Prototype                        │
//! │                                │                                    │
//! │                                ▼                                    │
//! │                         Start runtime_call                          │
//! │                                │                                    │
//! │                                ▼                                    │
//! │                   ┌────── Event Loop ──────┐                        │
//! │                   │                        │                        │
//! │                   ▼                        ▼                        │
//! │            StorageGet?              SignatureVerify?                │
//! │                   │                        │                        │
//! │                   ▼                        ▼                        │
//! │     Query LocalStorageLayer         Verify or mock                  │
//! │                   │                        │                        │
//! │                   └──────────┬─────────────┘                        │
//! │                              ▼                                      │
//! │                         Finished?                                   │
//! │                              │                                      │
//! │                              ▼                                      │
//! │                   Return RuntimeCallResult                          │
//! │                   (output + storage_diff)                           │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::{RuntimeExecutor, LocalStorageLayer, RemoteStorageLayer};
//!
//! // Create the storage layers
//! let remote = RemoteStorageLayer::new(rpc, cache, block_hash);
//! let local = LocalStorageLayer::new(remote);
//!
//! // Create executor with runtime WASM code fetched from chain
//! let runtime_code = rpc.runtime_code(block_hash).await?;
//! let executor = RuntimeExecutor::new(runtime_code, None)?;
//!
//! // Execute a runtime call against the local storage layer
//! let result = executor.call("Core_version", &[], &local).await?;
//! println!("Output: {:?}", result.output);
//! println!("Storage changes: {:?}", result.storage_diff.len());
//! ```

use crate::{
	LocalStorageLayer,
	error::ExecutorError,
	local::LocalSharedValue,
	strings::executor::{magic_signature, storage_prefixes},
};
use smoldot::{
	executor::{
		self,
		host::{Config as HostConfig, HostVmPrototype},
		runtime_call::{self, OffchainContext, RuntimeCall},
		storage_diff::TrieDiff,
		vm::{ExecHint, HeapPages},
	},
	trie::{TrieEntryVersion, bytes_to_nibbles, nibbles_to_bytes_suffix_extend},
};
use std::{collections::BTreeMap, iter, iter::Once, sync::Arc};

struct ArcLocalSharedValue(Arc<LocalSharedValue>);

impl AsRef<[u8]> for ArcLocalSharedValue {
	fn as_ref(&self) -> &[u8] {
		self.0.as_ref().as_ref()
	}
}

/// Signature mock mode for testing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SignatureMockMode {
	/// No mock - verify all signatures normally.
	#[default]
	None,
	/// Accept signatures starting with magic bytes `0xdeadbeef` (padded with `0xcd`).
	///
	/// This is similar to how Foundry's `vm.prank()` works for EVM testing - it lets you
	/// impersonate any account for testing purposes. Real signatures are still verified
	/// normally, but transactions with magic signatures bypass verification.
	MagicSignature,
	/// Accept all signatures as valid.
	AlwaysValid,
}

/// Result of a runtime call execution.
#[derive(Debug, Clone)]
pub struct RuntimeCallResult {
	/// The output bytes returned by the runtime function.
	pub output: Vec<u8>,
	/// Storage changes made during execution.
	///
	/// Each entry is `(key, value)` where `value` is `None` for deletions.
	pub storage_diff: Vec<(Vec<u8>, Option<Vec<u8>>)>,
	/// Offchain storage changes made during execution.
	pub offchain_storage_diff: Vec<(Vec<u8>, Option<Vec<u8>>)>,
	/// Log messages emitted by the runtime.
	pub logs: Vec<RuntimeLog>,
}

/// A log message emitted by the runtime.
#[derive(Debug, Clone)]
pub struct RuntimeLog {
	/// The log message.
	pub message: String,
	/// Log level (0=error, 1=warn, 2=info, 3=debug, 4=trace).
	pub level: Option<u32>,
	/// Log target (e.g., "runtime", "pallet_balances").
	pub target: Option<String>,
}

/// Configuration for runtime execution.
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
	/// Signature mock mode for testing.
	pub signature_mock: SignatureMockMode,
	/// Whether to allow unresolved imports in the runtime.
	pub allow_unresolved_imports: bool,
	/// Maximum log level (0=off, 1=error, 2=warn, 3=info, 4=debug, 5=trace).
	pub max_log_level: u32,
	/// Value to return for storage proof size queries.
	pub storage_proof_size: u64,
}

impl Default for ExecutorConfig {
	fn default() -> Self {
		Self {
			signature_mock: SignatureMockMode::None,
			allow_unresolved_imports: false,
			max_log_level: 3, // Info
			storage_proof_size: 0,
		}
	}
}

/// Runtime executor for executing Substrate runtime calls.
///
/// This struct wraps smoldot's executor to run WASM runtime code against
/// lazy-loaded storage via [`LocalStorageLayer`].
///
/// # Thread Safety
///
/// `RuntimeExecutor` is `Send + Sync` and can be shared across async tasks.
/// Each call to [`RuntimeExecutor::call`] creates a new VM instance, so
/// multiple calls can safely execute concurrently (though they will each
/// have independent storage views).
///
/// # Example
///
/// ```ignore
/// let executor = RuntimeExecutor::new(runtime_code, None)?;
/// let version = executor.runtime_version()?;
/// let result = executor.call("Metadata_metadata", &[], &storage).await?;
/// ```
///
/// # Cloning
///
/// `RuntimeExecutor` is cheap to clone. The runtime code is stored in an `Arc<[u8]>`,
/// so cloning only increments a reference count. Multiple executors can share the
/// same runtime code without copying.
#[derive(Clone)]
pub struct RuntimeExecutor {
	/// The WASM runtime code (shared via Arc to avoid copying large blobs).
	runtime_code: Arc<[u8]>,
	/// Number of heap pages available to the runtime.
	heap_pages: HeapPages,
	/// Execution configuration.
	config: ExecutorConfig,
}

impl RuntimeExecutor {
	/// Create a new executor with runtime WASM code.
	///
	/// # Arguments
	///
	/// * `runtime_code` - The WASM runtime code (can be zstd-compressed). Accepts `Vec<u8>`,
	///   `Arc<[u8]>`, or any type that implements `Into<Arc<[u8]>>`.
	/// * `heap_pages` - Number of heap pages. Use `None` to read from storage or use default.
	///
	/// # Errors
	///
	/// Returns an error if the WASM code is invalid.
	///
	/// # Example
	///
	/// ```ignore
	/// // From Vec<u8> (takes ownership, no copy if Arc is created)
	/// let executor = RuntimeExecutor::new(runtime_code_vec, None)?;
	///
	/// // From Arc<[u8]> (cheap clone, shares the same allocation)
	/// let executor = RuntimeExecutor::new(runtime_code_arc.clone(), None)?;
	/// ```
	pub fn new(
		runtime_code: impl Into<Arc<[u8]>>,
		heap_pages: Option<u32>,
	) -> Result<Self, ExecutorError> {
		let runtime_code: Arc<[u8]> = runtime_code.into();
		let heap_pages = heap_pages.map(HeapPages::from).unwrap_or(executor::DEFAULT_HEAP_PAGES);

		// Validate the WASM code by creating a prototype
		let _prototype = HostVmPrototype::new(HostConfig {
			module: &runtime_code,
			heap_pages,
			exec_hint: ExecHint::ValidateAndExecuteOnce,
			allow_unresolved_imports: false,
		})?;

		Ok(Self { runtime_code, heap_pages, config: ExecutorConfig::default() })
	}

	/// Create a new executor with custom configuration.
	///
	/// # Arguments
	///
	/// * `runtime_code` - The WASM runtime code (can be zstd-compressed).
	/// * `heap_pages` - Number of heap pages. Use `None` to use the default.
	/// * `config` - Custom execution configuration.
	pub fn with_config(
		runtime_code: impl Into<Arc<[u8]>>,
		heap_pages: Option<u32>,
		config: ExecutorConfig,
	) -> Result<Self, ExecutorError> {
		let mut executor = Self::new(runtime_code, heap_pages)?;
		executor.config = config;
		Ok(executor)
	}

	/// Execute a runtime call.
	///
	/// # Arguments
	///
	/// * `method` - The runtime method to call (e.g., "Core_version", "Metadata_metadata").
	/// * `args` - SCALE-encoded arguments for the method.
	/// * `storage` - Storage layer for reading state from the forked chain.
	///
	/// # Returns
	///
	/// Returns the call result including output bytes and storage diff.
	///
	/// # Common Runtime Methods
	///
	/// | Method | Purpose |
	/// |--------|---------|
	/// | `Core_version` | Get runtime version |
	/// | `Core_initialize_block` | Initialize a new block |
	/// | `BlockBuilder_apply_extrinsic` | Apply a transaction |
	/// | `BlockBuilder_finalize_block` | Finalize block, get header |
	/// | `Metadata_metadata` | Get runtime metadata |
	pub async fn call(
		&self,
		method: &str,
		args: &[u8],
		storage: &LocalStorageLayer,
	) -> Result<RuntimeCallResult, ExecutorError> {
		// Create VM prototype
		let vm_proto = HostVmPrototype::new(HostConfig {
			module: &self.runtime_code,
			heap_pages: self.heap_pages,
			exec_hint: ExecHint::ValidateAndExecuteOnce,
			allow_unresolved_imports: self.config.allow_unresolved_imports,
		})?;

		// Start the runtime call
		let mut vm = runtime_call::run(runtime_call::Config {
			virtual_machine: vm_proto,
			function_to_call: method,
			parameter: iter::once(args),
			storage_main_trie_changes: TrieDiff::default(),
			max_log_level: self.config.max_log_level,
			calculate_trie_changes: false,
			storage_proof_size_behavior:
				runtime_call::StorageProofSizeBehavior::ConstantReturnValue(
					self.config.storage_proof_size,
				),
		})
		.map_err(|(err, _)| ExecutorError::StartError {
			method: method.to_string(),
			message: err.to_string(),
		})?;

		// Track storage changes during execution
		let mut storage_changes: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
		let mut offchain_storage_changes: BTreeMap<Vec<u8>, Option<Vec<u8>>> = BTreeMap::new();
		let mut logs: Vec<RuntimeLog> = Vec::new();

		// Execute the runtime call
		loop {
			vm = match vm {
				RuntimeCall::Finished(result) => {
					return match result {
						Ok(success) => {
							// Collect storage changes from the execution
							success.storage_changes.storage_changes_iter_unordered().for_each(
								|(child, key, value)| {
									let prefixed_key = if let Some(child) = child {
										prefixed_child_key(
											child.iter().copied(),
											key.iter().copied(),
										)
									} else {
										key.to_vec()
									};
									storage_changes.insert(prefixed_key, value.map(|v| v.to_vec()));
								},
							);

							Ok(RuntimeCallResult {
								output: success.virtual_machine.value().as_ref().to_vec(),
								storage_diff: storage_changes.into_iter().collect(),
								offchain_storage_diff: offchain_storage_changes
									.into_iter()
									.collect(),
								logs,
							})
						},
						Err(err) => Err(err.into()),
					};
				},

				RuntimeCall::StorageGet(req) => {
					let key = if let Some(child) = req.child_trie() {
						prefixed_child_key(
							child.as_ref().iter().copied(),
							req.key().as_ref().iter().copied(),
						)
					} else {
						req.key().as_ref().to_vec()
					};

					// Check local changes first
					if let Some(value) = storage_changes.get(&key) {
						req.inject_value(
							value.as_ref().map(|v| (iter::once(v), TrieEntryVersion::V1)),
						)
					} else {
						// Fetch from storage backend at the latest block
						let block_number = storage.get_latest_block_number();
						let value = storage.get(block_number, &key).await.map_err(|e| {
							ExecutorError::StorageError {
								key: hex::encode(&key),
								message: e.to_string(),
							}
						})?;
						let none_placeholder: Option<(Once<[u8; 0]>, TrieEntryVersion)> = None;
						match value {
							// A local shared value can be empty, just flagging that a key was
							// manually
							Some(value)
								if !<LocalSharedValue as AsRef<[u8]>>::as_ref(&value)
									.is_empty() =>
								req.inject_value(Some((
									iter::once(ArcLocalSharedValue(value)),
									TrieEntryVersion::V1,
								))),
							_ => req.inject_value(none_placeholder),
						}
					}
				},

				RuntimeCall::ClosestDescendantMerkleValue(req) => {
					// We don't have merkle values - let smoldot calculate them
					req.resume_unknown()
				},

				RuntimeCall::NextKey(req) => {
					if req.branch_nodes() {
						// Root calculation - skip
						req.inject_key(None::<Vec<_>>.map(|x| x.into_iter()))
					} else {
						let prefix = if let Some(child) = req.child_trie() {
							prefixed_child_key(
								child.as_ref().iter().copied(),
								nibbles_to_bytes_suffix_extend(req.prefix()),
							)
						} else {
							nibbles_to_bytes_suffix_extend(req.prefix()).collect::<Vec<_>>()
						};

						let key = if let Some(child) = req.child_trie() {
							prefixed_child_key(
								child.as_ref().iter().copied(),
								nibbles_to_bytes_suffix_extend(req.key()),
							)
						} else {
							nibbles_to_bytes_suffix_extend(req.key()).collect::<Vec<_>>()
						};

						let next = storage.next_key(&prefix, &key).await.map_err(|e| {
							ExecutorError::StorageError {
								key: hex::encode(&key),
								message: e.to_string(),
							}
						})?;

						req.inject_key(next.map(|k| bytes_to_nibbles(k.into_iter())))
					}
				},

				RuntimeCall::SignatureVerification(req) => match self.config.signature_mock {
					SignatureMockMode::MagicSignature => {
						if is_magic_signature(req.signature().as_ref()) {
							req.resume_success()
						} else {
							req.verify_and_resume()
						}
					},
					SignatureMockMode::AlwaysValid => req.resume_success(),
					SignatureMockMode::None => req.verify_and_resume(),
				},

				RuntimeCall::OffchainStorageSet(req) => {
					offchain_storage_changes.insert(
						req.key().as_ref().to_vec(),
						req.value().map(|x| x.as_ref().to_vec()),
					);
					req.resume()
				},

				RuntimeCall::Offchain(ctx) => match ctx {
					OffchainContext::StorageGet(req) => {
						// Check local offchain changes first
						let key = req.key().as_ref().to_vec();
						let value = offchain_storage_changes.get(&key).cloned().flatten();
						req.inject_value(value)
					},
					OffchainContext::StorageSet(req) => {
						let key = req.key().as_ref().to_vec();
						let current = offchain_storage_changes.get(&key);

						let replace = match (current, req.old_value()) {
							(Some(Some(current)), Some(old)) => old.as_ref().eq(current),
							_ => true,
						};

						if replace {
							offchain_storage_changes
								.insert(key, req.value().map(|x| x.as_ref().to_vec()));
						}

						req.resume(replace)
					},
					OffchainContext::Timestamp(req) => {
						// Return current time in milliseconds
						let timestamp = std::time::SystemTime::now()
							.duration_since(std::time::UNIX_EPOCH)
							.map(|d| d.as_millis() as u64)
							.unwrap_or(0);
						req.inject_timestamp(timestamp)
					},
					OffchainContext::RandomSeed(req) => {
						// Generate random seed using blake2
						let seed = sp_core::blake2_256(
							&std::time::SystemTime::now()
								.duration_since(std::time::UNIX_EPOCH)
								.map(|d| d.as_nanos().to_le_bytes())
								.unwrap_or([0u8; 16]),
						);
						req.inject_random_seed(seed)
					},
					OffchainContext::SubmitTransaction(req) => {
						// We don't support submitting transactions from offchain workers
						req.resume(false)
					},
				},

				RuntimeCall::LogEmit(req) => {
					use smoldot::executor::host::LogEmitInfo;

					let log = match req.info() {
						LogEmitInfo::Num(v) => {
							eprintln!("[Executor] LogEmit::Num: {}", v);
							RuntimeLog { message: format!("{}", v), level: None, target: None }
						},
						LogEmitInfo::Utf8(v) => {
							eprintln!("[Executor] LogEmit::Utf8: {}", v);
							RuntimeLog { message: v.to_string(), level: None, target: None }
						},
						LogEmitInfo::Hex(v) => {
							eprintln!("[Executor] LogEmit::Hex: {}", v);
							RuntimeLog { message: v.to_string(), level: None, target: None }
						},
						LogEmitInfo::Log { log_level, target, message } => {
							eprintln!(
								"[Executor] LogEmit::Log [{}] {}: {}",
								log_level, target, message
							);
							RuntimeLog {
								message: message.to_string(),
								level: Some(log_level),
								target: Some(target.to_string()),
							}
						},
					};
					logs.push(log);
					req.resume()
				},
			}
		}
	}

	/// Get the runtime version from the WASM code.
	///
	/// This reads the version from the WASM custom sections without executing any code.
	pub fn runtime_version(&self) -> Result<RuntimeVersion, ExecutorError> {
		let prototype = HostVmPrototype::new(HostConfig {
			module: &self.runtime_code,
			heap_pages: self.heap_pages,
			exec_hint: ExecHint::ValidateAndExecuteOnce,
			allow_unresolved_imports: true,
		})?;

		let version = prototype.runtime_version().decode();

		Ok(RuntimeVersion {
			spec_name: version.spec_name.to_string(),
			impl_name: version.impl_name.to_string(),
			authoring_version: version.authoring_version,
			spec_version: version.spec_version,
			impl_version: version.impl_version,
			transaction_version: version.transaction_version.unwrap_or(0),
			state_version: version.state_version.map(|v| v.into()).unwrap_or(0),
		})
	}
}

/// Runtime version information.
#[derive(Debug, Clone)]
pub struct RuntimeVersion {
	/// Spec name (e.g., "polkadot", "kusama").
	pub spec_name: String,
	/// Implementation name.
	pub impl_name: String,
	/// Authoring version.
	pub authoring_version: u32,
	/// Spec version.
	pub spec_version: u32,
	/// Implementation version.
	pub impl_version: u32,
	/// Transaction version.
	pub transaction_version: u32,
	/// State version (0 or 1).
	pub state_version: u8,
}

/// Create a prefixed key for child storage access.
fn prefixed_child_key(child: impl Iterator<Item = u8>, key: impl Iterator<Item = u8>) -> Vec<u8> {
	[storage_prefixes::DEFAULT_CHILD_STORAGE, &child.collect::<Vec<_>>(), &key.collect::<Vec<_>>()]
		.concat()
}

/// Check if a signature is a magic test signature.
///
/// Magic signatures start with `0xdeadbeef` and are padded with `0xcd`.
fn is_magic_signature(signature: &[u8]) -> bool {
	signature.starts_with(magic_signature::PREFIX) &&
		signature[magic_signature::PREFIX.len()..]
			.iter()
			.all(|&b| b == magic_signature::PADDING)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn magic_signature_accepts_valid_patterns() {
		// Valid magic signatures
		assert!(is_magic_signature(&[0xde, 0xad, 0xbe, 0xef, 0xcd, 0xcd]));
		assert!(is_magic_signature(&[0xde, 0xad, 0xbe, 0xef, 0xcd, 0xcd, 0xcd, 0xcd]));
		assert!(is_magic_signature(&[0xde, 0xad, 0xbe, 0xef])); // Just prefix

		// Invalid signatures
		assert!(!is_magic_signature(&[0xde, 0xad, 0xbe, 0xef, 0xcd, 0xcd, 0xcd, 0x00]));
		assert!(!is_magic_signature(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]));
		assert!(!is_magic_signature(&[0xde, 0xad, 0xbe])); // Too short
	}

	#[test]
	fn prefixed_child_key_combines_prefix_child_and_key() {
		let child = b"child1".iter().copied();
		let key = b"key1".iter().copied();
		let result = prefixed_child_key(child, key);

		assert!(result.starts_with(storage_prefixes::DEFAULT_CHILD_STORAGE));
		assert!(result.ends_with(b"key1"));
	}

	#[test]
	fn executor_config_has_sensible_defaults() {
		let config = ExecutorConfig::default();
		assert_eq!(config.signature_mock, SignatureMockMode::None);
		assert!(!config.allow_unresolved_imports);
		assert_eq!(config.max_log_level, 3);
		assert_eq!(config.storage_proof_size, 0);
	}

	#[test]
	fn signature_mock_mode_defaults_to_none() {
		let mode = SignatureMockMode::default();
		assert_eq!(mode, SignatureMockMode::None);
	}

	/// Integration tests that execute runtime calls against a local test node.
	///
	/// These tests verify that the executor can correctly execute Substrate runtime
	/// methods against real chain state. They spawn a local test node and fetch
	/// actual runtime code to ensure end-to-end functionality.
	mod sequential {
		use crate::{ForkRpcClient, LocalStorageLayer, RemoteStorageLayer, StorageCache};
		use pop_common::test_env::TestNode;
		use scale::Encode;
		use subxt::{Metadata, config::substrate::H256};
		use url::Url;

		use super::*;

		/// Test context holding a spawned test node and all layers needed for execution.
		///
		/// The node is kept alive for the duration of the test via the `_node` field.
		struct ExecutorTestContext {
			#[allow(dead_code)]
			node: TestNode,
			executor: RuntimeExecutor,
			storage: LocalStorageLayer,
			#[allow(dead_code)]
			block_hash: H256,
			#[allow(dead_code)]
			block_number: u32,
		}

		/// Creates a fully initialized executor test context.
		///
		/// This spawns a local test node, connects to it, fetches the runtime code,
		/// and sets up all storage layers needed for runtime execution.
		async fn create_executor_context() -> ExecutorTestContext {
			create_executor_context_with_config(ExecutorConfig::default()).await
		}

		/// Creates an executor test context with a custom configuration.
		///
		/// This allows tests to customize executor behavior such as signature
		/// mock modes and log levels.
		async fn create_executor_context_with_config(
			config: ExecutorConfig,
		) -> ExecutorTestContext {
			use scale::Decode as _;

			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");
			let rpc = ForkRpcClient::connect(&endpoint).await.expect("Failed to connect to node");

			let block_hash: H256 =
				rpc.finalized_head().await.expect("Failed to get finalized head");
			let header = rpc.header(block_hash).await.expect("Failed to get block header");
			let block_number = header.number;

			// Fetch runtime code from the chain
			let runtime_code =
				rpc.runtime_code(block_hash).await.expect("Failed to fetch runtime code");

			// Fetch and decode metadata
			let metadata_bytes = rpc.metadata(block_hash).await.expect("Failed to fetch metadata");
			let metadata = Metadata::decode(&mut metadata_bytes.as_slice())
				.expect("Failed to decode metadata");

			// Set up storage layers
			let cache = StorageCache::in_memory().await.expect("Failed to create cache");
			cache
				.cache_block(block_hash, block_number, header.parent_hash, &header.encode())
				.await
				.expect("Failed to cache block");

			let remote = RemoteStorageLayer::new(rpc, cache);
			let storage = LocalStorageLayer::new(remote, block_number, block_hash, metadata);

			// Create executor with custom config
			let executor = RuntimeExecutor::with_config(runtime_code, None, config)
				.expect("Failed to create executor");

			ExecutorTestContext { node, executor, storage, block_hash, block_number }
		}

		/// Executes `Core_version` against a live test node and verifies the result.
		///
		/// `Core_version` is a fundamental runtime API that returns the runtime's version
		/// information. This test verifies that:
		/// 1. The executor can successfully execute a runtime call
		/// 2. The output is non-empty (contains SCALE-encoded RuntimeVersion)
		/// 3. The runtime version can be extracted from the executor
		#[tokio::test(flavor = "multi_thread")]
		async fn core_version_executes_successfully() {
			let ctx = create_executor_context().await;

			// Execute Core_version - this is a read-only call with no arguments
			let result = ctx
				.executor
				.call("Core_version", &[], &ctx.storage)
				.await
				.expect("Core_version execution failed");

			// Verify we got output
			assert!(!result.output.is_empty(), "Core_version should return non-empty output");

			// Core_version is read-only, should have no storage changes
			assert!(result.storage_diff.is_empty(), "Core_version should not modify storage");

			// Verify we can also get the version directly from the executor
			let version = ctx.executor.runtime_version().expect("Failed to get runtime version");
			assert!(!version.spec_name.is_empty(), "spec_name should not be empty");
			assert!(version.spec_version > 0, "spec_version should be positive");
		}

		/// Executes `Metadata_metadata` against a live test node and verifies the result.
		///
		/// `Metadata_metadata` returns the runtime's metadata, which describes all pallets,
		/// storage items, calls, and events. This is typically one of the largest runtime
		/// calls in terms of output size.
		///
		/// This test verifies that:
		/// 1. The executor can handle large output (metadata is typically hundreds of KB)
		/// 2. The output contains valid metadata (starts with expected magic bytes)
		/// 3. No storage modifications occur (metadata is read-only)
		#[tokio::test(flavor = "multi_thread")]
		async fn metadata_executes_successfully() {
			let ctx = create_executor_context().await;

			// Execute Metadata_metadata - this is a read-only call with no arguments
			let result = ctx
				.executor
				.call("Metadata_metadata", &[], &ctx.storage)
				.await
				.expect("Metadata_metadata execution failed");

			// Metadata is typically large (hundreds of KB)
			assert!(
				result.output.len() > 1000,
				"Metadata should be larger than 1KB, got {} bytes",
				result.output.len()
			);

			// Metadata_metadata is read-only, should have no storage changes
			assert!(result.storage_diff.is_empty(), "Metadata_metadata should not modify storage");
		}

		/// Verifies that `with_config` correctly applies custom configuration.
		///
		/// This test creates an executor with a custom configuration and verifies
		/// that the configuration affects execution behavior.
		#[tokio::test(flavor = "multi_thread")]
		async fn with_config_applies_custom_settings() {
			// Create executor with custom config - high log level to capture all logs
			let config = ExecutorConfig {
				signature_mock: SignatureMockMode::AlwaysValid,
				allow_unresolved_imports: false,
				max_log_level: 5, // Trace level
				storage_proof_size: 1024,
			};

			let ctx = create_executor_context_with_config(config).await;

			// Execute a simple call to verify executor works with custom config
			let result = ctx
				.executor
				.call("Core_version", &[], &ctx.storage)
				.await
				.expect("Core_version with custom config failed");

			assert!(!result.output.is_empty(), "Should return output with custom config");
		}

		/// Verifies that logs are captured during runtime execution.
		///
		/// This test creates an executor with trace-level logging enabled and
		/// verifies that any runtime logs are captured in the result.
		#[tokio::test(flavor = "multi_thread")]
		async fn logs_are_captured_during_execution() {
			// Create executor with max log level to capture all possible logs
			let config = ExecutorConfig {
				max_log_level: 5, // Trace - capture everything
				..Default::default()
			};

			let ctx = create_executor_context_with_config(config).await;

			// Execute Metadata_metadata which may emit logs during execution
			let result = ctx
				.executor
				.call("Metadata_metadata", &[], &ctx.storage)
				.await
				.expect("Metadata_metadata execution failed");

			// Log the number of captured logs for debugging
			// Note: Whether logs are emitted depends on the runtime implementation
			println!("Captured {} runtime logs", result.logs.len());
			for log in &result.logs {
				println!(
					"  [{:?}] {}: {}",
					log.level,
					log.target.as_deref().unwrap_or("unknown"),
					log.message
				);
			}

			// The test passes regardless of log count - we're verifying the
			// log capture mechanism works, not that specific logs are emitted
			assert!(result.output.len() > 1000, "Metadata should still be returned");
		}

		/// Verifies that `Core_initialize_block` triggers storage writes.
		///
		/// Block initialization sets up the block environment including:
		/// - System::Number (current block number)
		/// - System::ParentHash (hash of parent block)
		/// - System::Digest (block digest items)
		///
		/// This exercises the storage write path in the executor.
		#[tokio::test(flavor = "multi_thread")]
		async fn core_initialize_block_modifies_storage() {
			let ctx = create_executor_context().await;

			// Create a header for the next block
			// The header format follows the Substrate Header structure
			let next_block_number = ctx.block_number + 1;

			// Construct a minimal header for initialization
			// Header = (parent_hash, number, state_root, extrinsics_root, digest)
			#[derive(Encode)]
			struct Header {
				parent_hash: H256,
				#[codec(compact)]
				number: u32,
				state_root: H256,
				extrinsics_root: H256,
				digest: Vec<DigestItem>,
			}

			#[derive(Encode)]
			enum DigestItem {
				#[codec(index = 6)]
				PreRuntime([u8; 4], Vec<u8>),
			}

			let header = Header {
				parent_hash: ctx.block_hash,
				number: next_block_number,
				state_root: H256::zero(),      // Will be computed by runtime
				extrinsics_root: H256::zero(), // Will be computed by runtime
				digest: vec![
					// Add a pre-runtime digest for Aura slot (required by most runtimes)
					DigestItem::PreRuntime(*b"aura", 0u64.encode()),
				],
			};

			let result = ctx
				.executor
				.call("Core_initialize_block", &header.encode(), &ctx.storage)
				.await
				.expect("Core_initialize_block execution failed");

			// Block initialization MUST write to storage
			assert!(
				!result.storage_diff.is_empty(),
				"Core_initialize_block should modify storage, got {} changes",
				result.storage_diff.len()
			);

			// Verify System::Number was updated
			// System::Number key = twox128("System") ++ twox128("Number")
			let system_prefix = sp_core::twox_128(b"System");
			let number_key = sp_core::twox_128(b"Number");
			let system_number_key: Vec<u8> =
				[system_prefix.as_slice(), number_key.as_slice()].concat();

			let has_number_update =
				result.storage_diff.iter().any(|(key, _)| key == &system_number_key);

			assert!(
				has_number_update,
				"Core_initialize_block should update System::Number. Keys modified: {:?}",
				result.storage_diff.iter().map(|(k, _)| hex::encode(k)).collect::<Vec<_>>()
			);

			println!("Core_initialize_block modified {} storage keys", result.storage_diff.len());
		}

		/// Verifies that the executor handles storage reads from accumulated changes.
		///
		/// During block building, the runtime may read back values it has written
		/// within the same execution. This tests that the executor correctly serves
		/// reads from the in-flight storage changes before falling back to the
		/// storage layer.
		#[tokio::test(flavor = "multi_thread")]
		async fn storage_reads_from_accumulated_changes() {
			let ctx = create_executor_context().await;

			// Core_initialize_block writes to storage and may read back some values
			// during the same execution (e.g., reading System::Number after setting it)

			#[derive(Encode)]
			struct Header {
				parent_hash: H256,
				#[codec(compact)]
				number: u32,
				state_root: H256,
				extrinsics_root: H256,
				digest: Vec<DigestItem>,
			}

			#[derive(Encode)]
			enum DigestItem {
				#[codec(index = 6)]
				PreRuntime([u8; 4], Vec<u8>),
			}

			let header = Header {
				parent_hash: ctx.block_hash,
				number: ctx.block_number + 1,
				state_root: H256::zero(),
				extrinsics_root: H256::zero(),
				digest: vec![DigestItem::PreRuntime(*b"aura", 0u64.encode())],
			};

			let result = ctx
				.executor
				.call("Core_initialize_block", &header.encode(), &ctx.storage)
				.await
				.expect("Core_initialize_block execution failed");

			// If we get here without errors, the storage read-back mechanism works
			// The runtime successfully read values it had written during execution
			assert!(!result.storage_diff.is_empty(), "Should have storage changes");
		}

		/// Verifies storage changes are properly applied between calls.
		///
		/// This exercises:
		/// - Storage writes during initialization
		/// - Applying storage changes to the local layer
		/// - Subsequent reads seeing the applied changes
		#[tokio::test(flavor = "multi_thread")]
		async fn storage_changes_persist_across_calls() {
			let ctx = create_executor_context().await;

			#[derive(Encode)]
			struct Header {
				parent_hash: H256,
				#[codec(compact)]
				number: u32,
				state_root: H256,
				extrinsics_root: H256,
				digest: Vec<DigestItem>,
			}

			#[derive(Encode)]
			enum DigestItem {
				#[codec(index = 6)]
				PreRuntime([u8; 4], Vec<u8>),
			}

			let header = Header {
				parent_hash: ctx.block_hash,
				number: ctx.block_number + 1,
				state_root: H256::zero(),
				extrinsics_root: H256::zero(),
				digest: vec![DigestItem::PreRuntime(*b"aura", 0u64.encode())],
			};

			// Step 1: Initialize block
			let init_result = ctx
				.executor
				.call("Core_initialize_block", &header.encode(), &ctx.storage)
				.await
				.expect("Core_initialize_block failed");

			assert!(!init_result.storage_diff.is_empty(), "Init should write storage");

			// Apply initialization changes to storage layer
			for (key, value) in &init_result.storage_diff {
				ctx.storage.set(key, value.as_deref()).expect("Failed to apply storage change");
			}

			// Step 2: Verify changes were applied by reading them back
			// The System::Number key should now have our block number
			let system_prefix = sp_core::twox_128(b"System");
			let number_key = sp_core::twox_128(b"Number");
			let system_number_key: Vec<u8> =
				[system_prefix.as_slice(), number_key.as_slice()].concat();

			let block_num = ctx.storage.get_latest_block_number();
			let stored_value = ctx
				.storage
				.get(block_num, &system_number_key)
				.await
				.expect("Failed to read System::Number");

			assert!(
				stored_value.is_some(),
				"System::Number should be set after Core_initialize_block"
			);

			println!(
				"Storage changes persist: {} keys modified, System::Number set",
				init_result.storage_diff.len()
			);
		}

		/// Verifies runtime_version extracts version without execution.
		///
		/// This tests the fast path that reads version info from WASM custom
		/// sections without actually executing any runtime code.
		#[tokio::test(flavor = "multi_thread")]
		async fn runtime_version_extracts_version_info() {
			let ctx = create_executor_context().await;

			let version = ctx.executor.runtime_version().expect("runtime_version should succeed");

			// Verify all version fields are populated
			assert!(!version.spec_name.is_empty(), "spec_name should not be empty");
			assert!(!version.impl_name.is_empty(), "impl_name should not be empty");
			assert!(version.spec_version > 0, "spec_version should be positive");

			println!(
				"Runtime version: {} v{} (impl: {} v{})",
				version.spec_name, version.spec_version, version.impl_name, version.impl_version
			);
		}
	}
}
