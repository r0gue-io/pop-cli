// SPDX-License-Identifier: GPL-3.0

//! Shared testing utilities for pop-fork tests.
//!
//! This module provides common test infrastructure used across different
//! modules in the crate to avoid duplication.

use crate::{
	Block, Blockchain, ExecutorConfig, ForkRpcClient, LocalStorageLayer, RemoteStorageLayer,
	RuntimeExecutor, StorageCache, TxPool,
	rpc_server::{ForkRpcServer, RpcServerConfig},
};
use pop_common::test_env::TestNode;
use scale::{Compact, Encode};
use sp_core::H256;
use std::sync::Arc;
use subxt::{Metadata, ext::codec::Decode};
use url::Url;

/// Alice's public key (Sr25519).
pub const ALICE: [u8; 32] = [
	0xd4, 0x35, 0x93, 0xc7, 0x15, 0xfd, 0xd3, 0x1c, 0x61, 0x14, 0x1a, 0xbd, 0x04, 0xa9, 0x9f, 0xd6,
	0x82, 0x2c, 0x85, 0x58, 0x85, 0x4c, 0xcd, 0xe3, 0x9a, 0x56, 0x84, 0xe7, 0xa5, 0x6d, 0xa2, 0x7d,
];

/// Bob's public key (Sr25519).
pub const BOB: [u8; 32] = [
	0x8e, 0xaf, 0x04, 0x15, 0x16, 0x87, 0x73, 0x63, 0x26, 0xc9, 0xfe, 0xa1, 0x7e, 0x25, 0xfc, 0x52,
	0x87, 0x61, 0x36, 0x93, 0xc9, 0x12, 0x90, 0x9c, 0xb2, 0x26, 0xaa, 0x47, 0x94, 0xf2, 0x6a, 0x48,
];

/// Compute Blake2_128Concat storage key for System::Account.
///
/// The key format is: twox128("System") ++ twox128("Account") ++ blake2_128(account) ++ account
pub fn account_storage_key(account: &[u8; 32]) -> Vec<u8> {
	let mut key = Vec::new();
	key.extend(sp_core::twox_128(b"System"));
	key.extend(sp_core::twox_128(b"Account"));
	key.extend(sp_core::blake2_128(account));
	key.extend(account);
	key
}

/// Decode AccountInfo and extract free balance.
///
/// The AccountInfo struct has AccountData at offset 16 bytes (after nonce, consumers,
/// providers, sufficients). The free balance is the first u128 in AccountData.
pub fn decode_free_balance(data: &[u8]) -> u128 {
	const ACCOUNT_DATA_OFFSET: usize = 16;
	u128::from_le_bytes(
		data[ACCOUNT_DATA_OFFSET..ACCOUNT_DATA_OFFSET + 16]
			.try_into()
			.expect("need 16 bytes for u128"),
	)
}

/// Build a mock V4 signed extrinsic with dummy signature.
///
/// This creates a properly formatted extrinsic that works with signature mocking
/// (SignatureMockMode::AlwaysValid). The extrinsic is signed by ALICE.
///
/// # Arguments
///
/// * `call_data` - The encoded call data (pallet index + call index + args)
///
/// # Format
///
/// The extrinsic format is:
/// - Compact length prefix
/// - Version byte: 0x84 (signed + v4)
/// - Address: MultiAddress::Id(ALICE)
/// - Signature: 64 zero bytes (dummy)
/// - Signed extensions (immortal era, nonce 0, zero tip, no eth origin)
/// - Call data
pub fn build_mock_signed_extrinsic_v4(call_data: &[u8]) -> Vec<u8> {
	let mut inner = Vec::new();
	// Version byte: signed (0x80) + v4 (0x04) = 0x84
	inner.push(0x84);
	// Address: MultiAddress::Id variant (0x00) + 32-byte account
	inner.push(0x00);
	inner.extend(ALICE);
	// Signature: 64 bytes (dummy - works with AlwaysValid)
	inner.extend([0u8; 64]);
	// Extra params (signed extensions):
	inner.push(0x00); // CheckMortality: immortal
	inner.extend(Compact(0u64).encode()); // CheckNonce
	inner.extend(Compact(0u128).encode()); // ChargeTransactionPayment
	inner.push(0x00); // EthSetOrigin: None
	// Call data
	inner.extend(call_data);
	// Prefix with compact length
	let mut extrinsic = Compact(inner.len() as u32).encode();
	extrinsic.extend(inner);
	extrinsic
}

/// Comprehensive test context holding all components needed for testing.
///
/// This context is designed to be flexible - fields are populated based on
/// what the test needs. Use the appropriate `create_*` function to get
/// a context with the components you need.
pub struct TestContext {
	/// The spawned test node (kept alive for the duration of the test).
	pub node: TestNode,
	/// WebSocket endpoint URL.
	pub endpoint: Url,
	/// RPC client connected to the node.
	pub rpc: ForkRpcClient,
	/// Storage cache (in-memory).
	pub cache: StorageCache,
	/// Block hash of the fork point.
	pub block_hash: H256,
	/// Block number of the fork point.
	pub block_number: u32,
	/// Decoded metadata.
	pub metadata: Metadata,
	/// Runtime code (WASM blob).
	pub runtime_code: Vec<u8>,
	/// Remote storage layer.
	pub remote: RemoteStorageLayer,
}

impl TestContext {
	/// Create a new test context with all common components initialized.
	///
	/// This spawns a local test node and sets up all the infrastructure
	/// needed for testing: RPC client, cache, metadata, and remote storage.
	pub async fn new() -> Self {
		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");
		let rpc = ForkRpcClient::connect(&endpoint).await.expect("Failed to connect to node");

		let block_hash = rpc.finalized_head().await.expect("Failed to get finalized head");
		let header = rpc.header(block_hash).await.expect("Failed to get block header");
		let block_number = header.number;

		let cache = StorageCache::in_memory().await.expect("Failed to create cache");

		// Cache the block for tests that need it
		cache
			.cache_block(block_hash, block_number, header.parent_hash, &header.encode())
			.await
			.expect("Failed to cache block");

		let metadata_bytes = rpc.metadata(block_hash).await.expect("Failed to fetch metadata");
		let metadata =
			Metadata::decode(&mut metadata_bytes.as_slice()).expect("Failed to decode metadata");

		let runtime_code =
			rpc.runtime_code(block_hash).await.expect("Failed to fetch runtime code");

		let remote = RemoteStorageLayer::new(rpc.clone(), cache.clone());

		TestContext {
			node,
			endpoint,
			rpc,
			cache,
			block_hash,
			block_number,
			metadata,
			runtime_code,
			remote,
		}
	}

	/// Create a LocalStorageLayer from this context.
	pub fn create_local_layer(&self) -> LocalStorageLayer {
		LocalStorageLayer::new(
			self.remote.clone(),
			self.block_number,
			self.block_hash,
			self.metadata.clone(),
		)
	}

	/// Create a RuntimeExecutor with default configuration.
	pub fn create_executor(&self) -> RuntimeExecutor {
		RuntimeExecutor::new(self.runtime_code.clone(), None).expect("Failed to create executor")
	}

	/// Create a RuntimeExecutor with custom configuration.
	pub fn create_executor_with_config(&self, config: ExecutorConfig) -> RuntimeExecutor {
		RuntimeExecutor::with_config(self.runtime_code.clone(), None, config)
			.expect("Failed to create executor")
	}

	/// Create a fork point Block from this context.
	pub async fn create_block(&self) -> Block {
		Block::fork_point(&self.endpoint, self.cache.clone(), self.block_hash.into())
			.await
			.expect("Failed to create fork point block")
	}
}

/// Test context for RPC server tests.
///
/// This context spawns a test node, forks a blockchain from it, and starts
/// an RPC server. Use this for testing RPC method implementations.
pub struct RpcTestContext {
	/// The spawned test node (kept alive for the duration of the test).
	#[allow(dead_code)]
	pub node: TestNode,
	/// The RPC server (kept alive for the duration of the test).
	#[allow(dead_code)]
	pub server: ForkRpcServer,
	/// WebSocket URL of the RPC server.
	pub ws_url: String,
	/// The forked blockchain.
	pub blockchain: Arc<Blockchain>,
}

impl RpcTestContext {
	/// Create a new RPC test context with default configuration.
	pub async fn new() -> Self {
		Self::with_config(ExecutorConfig::default()).await
	}

	/// Create a new RPC test context with custom executor configuration.
	pub async fn with_config(config: ExecutorConfig) -> Self {
		let node = TestNode::spawn().await.expect("Failed to spawn test node");
		let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");

		let blockchain = Blockchain::fork_with_config(&endpoint, None, None, config)
			.await
			.expect("Failed to fork blockchain");
		let txpool = Arc::new(TxPool::new());

		let server = ForkRpcServer::start(blockchain.clone(), txpool, RpcServerConfig::default())
			.await
			.expect("Failed to start RPC server");

		let ws_url = server.ws_url();
		RpcTestContext { node, server, ws_url, blockchain }
	}
}
