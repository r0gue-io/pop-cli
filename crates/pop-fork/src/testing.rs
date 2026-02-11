// SPDX-License-Identifier: GPL-3.0

//! Test utilities for pop-fork integration tests.
//!
//! Provides shared test contexts, constants, and helper functions
//! to eliminate duplication across test modules.
//!
//! # Usage
//!
//! ```ignore
//! use crate::testing::{TestContext, constants, accounts, helpers};
//!
//! // Create a minimal context (just node + endpoint)
//! let ctx = TestContext::minimal().await;
//!
//! // Create a context for storage tests
//! let ctx = TestContext::for_storage().await;
//!
//! // Create a context for blockchain tests
//! let ctx = TestContext::for_blockchain().await;
//!
//! // Create a context for RPC server tests
//! let ctx = TestContext::for_rpc_server().await;
//!
//! // Use the builder for custom configurations
//! let ctx = TestContextBuilder::new()
//!     .with_rpc()
//!     .with_cache()
//!     .with_blockchain()
//!     .executor_config(ExecutorConfig { ... })
//!     .build()
//!     .await;
//! ```

use crate::{
	Blockchain, ExecutorConfig, ForkRpcClient, RemoteStorageLayer, RuntimeExecutor, StorageCache,
	TxPool,
	rpc_server::{ForkRpcServer, RpcServerConfig},
};
#[cfg(test)]
use pop_common::test_env::TestNode;
use std::sync::Arc;
use subxt::{Metadata, config::substrate::H256};
#[cfg(test)]
use tokio::sync::OnceCell;
use url::Url;

const SHARED_TEST_NODE_WS_URL: &str = "ws://127.0.0.1:9944";

#[cfg(test)]
static UNIT_TEST_NODE_WS_URL: OnceCell<String> = OnceCell::const_new();

#[cfg(test)]
async fn unit_test_node_ws_url() -> String {
	UNIT_TEST_NODE_WS_URL
		.get_or_init(|| async {
			let node = TestNode::spawn().await.expect("Failed to spawn shared test node");
			let ws_url = node.ws_url().to_string();
			let _ = Box::leak(Box::new(node));
			ws_url
		})
		.await
		.clone()
}

async fn resolve_test_endpoint() -> Url {
	if let Ok(ws_url) = std::env::var("POP_FORK_TEST_NODE_WS_URL") {
		return ws_url.parse().expect("POP_FORK_TEST_NODE_WS_URL must be a valid WebSocket URL");
	}

	let default_endpoint: Url = SHARED_TEST_NODE_WS_URL.parse().expect("Invalid WebSocket URL");
	if ForkRpcClient::connect(&default_endpoint).await.is_ok() {
		return default_endpoint;
	}

	#[cfg(test)]
	{
		unit_test_node_ws_url().await.parse().expect("Invalid unit test WebSocket URL")
	}

	#[cfg(not(test))]
	panic!(
		"No test node reachable at {}. Start one with `pop up ink-node --detach --skip-confirm` \
or set POP_FORK_TEST_NODE_WS_URL",
		SHARED_TEST_NODE_WS_URL
	);
}

/// Well-known storage keys for testing.
pub mod constants {
	/// System::Number storage key: twox128("System") ++ twox128("Number")
	pub const SYSTEM_NUMBER_KEY: &str =
		"26aa394eea5630e07c48ae0c9558cef702a5c1b19ab7a04f536c519aca4983ac";

	/// System::ParentHash storage key: twox128("System") ++ twox128("ParentHash")
	pub const SYSTEM_PARENT_HASH_KEY: &str =
		"26aa394eea5630e07c48ae0c9558cef734abf5cb34d6244378cddbf18e849d96";

	/// System pallet prefix: twox128("System")
	pub const SYSTEM_PALLET_PREFIX: &str = "26aa394eea5630e07c48ae0c9558cef7";

	/// Transfer amount: 100 units (with 12 decimals)
	pub const TRANSFER_AMOUNT: u128 = 100_000_000_000_000;
}

/// Well-known dev accounts for testing (re-exported from [`crate::dev`]).
pub mod accounts {
	pub use crate::dev::{ALICE, BOB};
}

/// Helper functions for testing.
pub mod helpers {
	use super::accounts::ALICE;
	use scale::{Compact, Encode};

	pub use crate::dev::account_storage_key;

	/// Decode AccountInfo and extract free balance.
	///
	/// The AccountInfo struct layout:
	/// - nonce: u32 (4 bytes)
	/// - consumers: u32 (4 bytes)
	/// - providers: u32 (4 bytes)
	/// - sufficients: u32 (4 bytes)
	/// - data.free: u128 (16 bytes) <-- what we extract
	/// - data.reserved: u128 (16 bytes)
	/// - data.frozen: u128 (16 bytes)
	/// - data.flags: u128 (16 bytes)
	pub fn decode_free_balance(data: &[u8]) -> u128 {
		const ACCOUNT_DATA_OFFSET: usize = 16;
		u128::from_le_bytes(
			data[ACCOUNT_DATA_OFFSET..ACCOUNT_DATA_OFFSET + 16]
				.try_into()
				.expect("need 16 bytes for u128"),
		)
	}

	/// Decode AccountInfo and extract account nonce.
	pub fn decode_account_nonce(data: &[u8]) -> u32 {
		u32::from_le_bytes(data[0..4].try_into().expect("need 4 bytes for u32"))
	}

	/// Build a mock V4 signed extrinsic with dummy signature (from Alice).
	///
	/// This creates a structurally valid extrinsic that works with
	/// `SignatureMockMode::AlwaysValid`.
	pub fn build_mock_signed_extrinsic_v4(call_data: &[u8]) -> Vec<u8> {
		build_mock_signed_extrinsic_v4_with_nonce(call_data, 0)
	}

	/// Build a mock V4 signed extrinsic with dummy signature and explicit nonce.
	///
	/// This is useful for test suites that submit multiple extrinsics from the same
	/// account to one long-lived chain instance.
	pub fn build_mock_signed_extrinsic_v4_with_nonce(call_data: &[u8], nonce: u64) -> Vec<u8> {
		let mut inner = Vec::new();
		inner.push(0x84); // Version: signed (0x80) + v4 (0x04)
		inner.push(0x00); // MultiAddress::Id variant
		inner.extend(ALICE);
		inner.extend([0u8; 64]); // Dummy signature (works with AlwaysValid)
		inner.push(0x00); // CheckMortality: immortal
		inner.extend(Compact(nonce).encode()); // CheckNonce
		inner.extend(Compact(0u128).encode()); // ChargeTransactionPayment
		// `EthSetOrigin` is a zero-sized extension in ink-node v0.47.0 (encodes to no bytes).
		inner.extend(call_data);
		let mut extrinsic = Compact(inner.len() as u32).encode();
		extrinsic.extend(inner);
		extrinsic
	}
}

/// Test context with optional components built on demand.
///
/// Use [`TestContextBuilder`] or the convenience constructors to create.
pub struct TestContext {
	/// WebSocket endpoint URL.
	pub endpoint: Url,
	/// RPC client (if requested).
	pub rpc: Option<ForkRpcClient>,
	/// Storage cache (if requested).
	pub cache: Option<StorageCache>,
	/// Finalized block hash (if requested).
	pub block_hash: Option<H256>,
	/// Finalized block number (if requested).
	pub block_number: Option<u32>,
	/// Chain metadata (if requested).
	pub metadata: Option<Metadata>,
	/// Remote storage layer (if requested).
	pub remote: Option<RemoteStorageLayer>,
	/// Blockchain instance (if requested).
	pub blockchain: Option<Arc<Blockchain>>,
	/// RPC server (if requested).
	pub server: Option<ForkRpcServer>,
	/// Transaction pool (if requested).
	pub txpool: Option<Arc<TxPool>>,
	/// Runtime executor (if requested).
	pub executor: Option<RuntimeExecutor>,
}

impl TestContext {
	/// Create a minimal context with just node + endpoint.
	pub async fn minimal() -> Self {
		TestContextBuilder::new().build().await
	}

	/// Create a context for RPC client tests (node + rpc).
	pub async fn for_rpc_client() -> Self {
		TestContextBuilder::new().with_rpc().build().await
	}

	/// Create a context for storage tests (node + rpc + cache + block info).
	pub async fn for_storage() -> Self {
		TestContextBuilder::new()
			.with_rpc()
			.with_cache()
			.with_block_info()
			.build()
			.await
	}

	/// Create a context for remote storage layer tests.
	pub async fn for_remote() -> Self {
		TestContextBuilder::new().with_remote().with_block_info().build().await
	}

	/// Create a context for local storage layer tests.
	pub async fn for_local() -> Self {
		TestContextBuilder::new()
			.with_remote()
			.with_block_info()
			.with_metadata()
			.build()
			.await
	}

	/// Create a context for blockchain tests.
	pub async fn for_blockchain() -> Self {
		TestContextBuilder::new().with_blockchain().build().await
	}

	/// Create a context for blockchain tests with custom executor config.
	pub async fn for_blockchain_with_config(config: ExecutorConfig) -> Self {
		TestContextBuilder::new()
			.with_blockchain()
			.executor_config(config)
			.build()
			.await
	}

	/// Create a context for RPC server tests.
	pub async fn for_rpc_server() -> Self {
		TestContextBuilder::new().with_server().build().await
	}

	/// Create a context for RPC server tests with custom executor config.
	pub async fn for_rpc_server_with_config(config: ExecutorConfig) -> Self {
		TestContextBuilder::new().with_server().executor_config(config).build().await
	}

	/// Create a context for executor tests.
	pub async fn for_executor() -> Self {
		TestContextBuilder::new().with_executor().build().await
	}

	/// Create a context for executor tests with custom config.
	pub async fn for_executor_with_config(config: ExecutorConfig) -> Self {
		TestContextBuilder::new().with_executor().executor_config(config).build().await
	}

	/// Get the RPC client (panics if not initialized).
	pub fn rpc(&self) -> &ForkRpcClient {
		self.rpc.as_ref().expect("rpc not initialized - use with_rpc()")
	}

	/// Get the storage cache (panics if not initialized).
	pub fn cache(&self) -> &StorageCache {
		self.cache.as_ref().expect("cache not initialized - use with_cache()")
	}

	/// Get the block hash (panics if not initialized).
	pub fn block_hash(&self) -> H256 {
		self.block_hash.expect("block_hash not initialized - use with_block_info()")
	}

	/// Get the block number (panics if not initialized).
	pub fn block_number(&self) -> u32 {
		self.block_number.expect("block_number not initialized - use with_block_info()")
	}

	/// Get the metadata (panics if not initialized).
	pub fn metadata(&self) -> &Metadata {
		self.metadata.as_ref().expect("metadata not initialized - use with_metadata()")
	}

	/// Get the remote storage layer (panics if not initialized).
	pub fn remote(&self) -> &RemoteStorageLayer {
		self.remote.as_ref().expect("remote not initialized - use with_remote()")
	}

	/// Get the blockchain (panics if not initialized).
	pub fn blockchain(&self) -> &Arc<Blockchain> {
		self.blockchain
			.as_ref()
			.expect("blockchain not initialized - use with_blockchain()")
	}

	/// Get the RPC server (panics if not initialized).
	pub fn server(&self) -> &ForkRpcServer {
		self.server.as_ref().expect("server not initialized - use with_server()")
	}

	/// Get the transaction pool (panics if not initialized).
	pub fn txpool(&self) -> &Arc<TxPool> {
		self.txpool.as_ref().expect("txpool not initialized - use with_server()")
	}

	/// Get the runtime executor (panics if not initialized).
	pub fn executor(&self) -> &RuntimeExecutor {
		self.executor.as_ref().expect("executor not initialized - use with_executor()")
	}

	/// Get the WebSocket URL for the RPC server (panics if not initialized).
	pub fn ws_url(&self) -> String {
		self.server().ws_url()
	}
}

/// Builder for [`TestContext`].
///
/// Allows configuring which components to initialize.
#[derive(Default)]
pub struct TestContextBuilder {
	executor_config: Option<ExecutorConfig>,
	with_rpc: bool,
	with_cache: bool,
	with_block_info: bool,
	with_metadata: bool,
	with_remote: bool,
	with_blockchain: bool,
	with_server: bool,
	with_executor: bool,
}

impl TestContextBuilder {
	/// Create a new builder with no components enabled.
	pub fn new() -> Self {
		Self::default()
	}

	/// Include RPC client in the context.
	pub fn with_rpc(mut self) -> Self {
		self.with_rpc = true;
		self
	}

	/// Include storage cache in the context.
	pub fn with_cache(mut self) -> Self {
		self.with_cache = true;
		self
	}

	/// Include block hash and number in the context.
	pub fn with_block_info(mut self) -> Self {
		self.with_block_info = true;
		self
	}

	/// Include metadata in the context.
	pub fn with_metadata(mut self) -> Self {
		self.with_metadata = true;
		self
	}

	/// Include remote storage layer (implies rpc + cache).
	pub fn with_remote(mut self) -> Self {
		self.with_remote = true;
		self.with_rpc = true;
		self.with_cache = true;
		self
	}

	/// Include blockchain instance.
	pub fn with_blockchain(mut self) -> Self {
		self.with_blockchain = true;
		self
	}

	/// Include RPC server (implies blockchain + txpool).
	pub fn with_server(mut self) -> Self {
		self.with_server = true;
		self.with_blockchain = true;
		self
	}

	/// Include runtime executor (implies rpc + cache + block_info).
	pub fn with_executor(mut self) -> Self {
		self.with_executor = true;
		self.with_rpc = true;
		self.with_cache = true;
		self.with_block_info = true;
		self
	}

	/// Set executor configuration (for signature mocking, etc.).
	pub fn executor_config(mut self, config: ExecutorConfig) -> Self {
		self.executor_config = Some(config);
		self
	}

	/// Build the test context.
	pub async fn build(self) -> TestContext {
		let endpoint = resolve_test_endpoint().await;

		// Initialize RPC client if needed
		let rpc = if self.with_rpc || self.with_block_info || self.with_metadata {
			Some(ForkRpcClient::connect(&endpoint).await.expect("Failed to connect RPC client"))
		} else {
			None
		};

		// Initialize cache if needed
		let cache = if self.with_cache {
			Some(StorageCache::in_memory().await.expect("Failed to create cache"))
		} else {
			None
		};

		// Get block info if needed
		let (block_hash, block_number) = if self.with_block_info {
			let rpc_ref = rpc.as_ref().expect("RPC required for block info");
			let hash = rpc_ref.finalized_head().await.expect("Failed to get finalized head");
			let header = rpc_ref.header(hash).await.expect("Failed to get header");
			(Some(hash), Some(header.number))
		} else {
			(None, None)
		};

		// Get metadata if needed
		let metadata = if self.with_metadata {
			let rpc_ref = rpc.as_ref().expect("RPC required for metadata");
			let hash = block_hash.unwrap_or_else(|| {
				panic!("block_hash required for metadata - use with_block_info()")
			});
			Some(rpc_ref.metadata(hash).await.expect("Failed to get metadata"))
		} else {
			None
		};

		// Create remote storage layer if needed
		let remote = if self.with_remote {
			let rpc_clone = rpc.clone().expect("RPC required for remote");
			let cache_clone = cache.clone().expect("Cache required for remote");
			Some(RemoteStorageLayer::new(rpc_clone, cache_clone))
		} else {
			None
		};

		// Create blockchain if needed
		let blockchain = if self.with_blockchain {
			let config = self.executor_config.clone().unwrap_or_default();
			Some(
				Blockchain::fork_with_config(&endpoint, None, None, config)
					.await
					.expect("Failed to fork blockchain"),
			)
		} else {
			None
		};

		// Create txpool if needed
		let txpool = if self.with_server { Some(Arc::new(TxPool::new())) } else { None };

		// Create RPC server if needed
		let server = if self.with_server {
			let blockchain_ref = blockchain.clone().expect("Blockchain required for server");
			let txpool_ref = txpool.clone().expect("TxPool required for server");
			Some(
				ForkRpcServer::start(blockchain_ref, txpool_ref, RpcServerConfig::default())
					.await
					.expect("Failed to start RPC server"),
			)
		} else {
			None
		};

		// Create executor if needed
		let executor = if self.with_executor {
			let rpc_ref = rpc.as_ref().expect("RPC required for executor");
			let hash = block_hash.expect("block_hash required for executor");

			// Fetch runtime code
			let code_key = sp_core::storage::well_known_keys::CODE;
			let runtime_code = rpc_ref
				.storage(code_key, hash)
				.await
				.expect("Failed to get runtime code")
				.expect("Runtime code not found");

			let config = self.executor_config.unwrap_or_default();
			Some(
				RuntimeExecutor::with_config(runtime_code, None, config)
					.expect("Failed to create executor"),
			)
		} else {
			None
		};

		TestContext {
			endpoint,
			rpc,
			cache,
			block_hash,
			block_number,
			metadata,
			remote,
			blockchain,
			server,
			txpool,
			executor,
		}
	}
}
