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
//! │                      RuntimeExecutor                                 │
//! │                                                                      │
//! │   call(method, args) ──► Create VM Prototype                        │
//! │                                │                                     │
//! │                                ▼                                     │
//! │                         Start runtime_call                          │
//! │                                │                                     │
//! │                                ▼                                     │
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
use std::{collections::BTreeMap, iter, sync::Arc};

/// Signature mock mode for testing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SignatureMockMode {
	/// No mock - verify all signatures normally.
	#[default]
	None,
	/// Accept signatures starting with magic bytes `0xdeadbeef` (padded with `0xcd`).
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
							value.clone().map(|x| (iter::once(x), TrieEntryVersion::V1)),
						)
					} else {
						// Fetch from storage backend
						let value =
							storage.get(&key).await.map_err(|e| ExecutorError::StorageError {
								key: hex::encode(&key),
								message: e.to_string(),
							})?;
						// Convert Arc<Vec<u8>> to Vec<u8> for inject_value
						req.inject_value(
							value.map(|arc| (iter::once((*arc).clone()), TrieEntryVersion::V1)),
						)
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
						LogEmitInfo::Num(v) =>
							RuntimeLog { message: format!("{}", v), level: None, target: None },
						LogEmitInfo::Utf8(v) =>
							RuntimeLog { message: v.to_string(), level: None, target: None },
						LogEmitInfo::Hex(v) =>
							RuntimeLog { message: v.to_string(), level: None, target: None },
						LogEmitInfo::Log { log_level, target, message } => RuntimeLog {
							message: message.to_string(),
							level: Some(log_level),
							target: Some(target.to_string()),
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
}
