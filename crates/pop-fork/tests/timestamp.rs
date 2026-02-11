// SPDX-License-Identifier: GPL-3.0

//! Integration tests for timestamp slot-duration detection on local and live chains.

#![cfg(feature = "integration-tests")]

use pop_common::test_env::TestNode;
use pop_fork::{
	ForkRpcClient, LocalStorageLayer, RemoteStorageLayer, RuntimeExecutor, StorageCache,
	TimestampInherent,
};
use subxt::Metadata;
use url::Url;

const DEFAULT_RELAY_SLOT_DURATION_MS: u64 = 6_000;
const DEFAULT_PARA_SLOT_DURATION_MS: u64 = 12_000;

/// Asset Hub Paseo endpoints (Aura-based parachain).
const ASSET_HUB_PASEO_ENDPOINTS: &[&str] = &[
	"wss://sys.ibp.network/asset-hub-paseo",
	"wss://sys.turboflakes.io/asset-hub-paseo",
	"wss://asset-hub-paseo.dotters.network",
];

/// Paseo relay chain endpoints (Babe-based chain).
const PASEO_RELAY_ENDPOINTS: &[&str] =
	&["wss://rpc.ibp.network/paseo", "wss://pas-rpc.stakeworld.io", "wss://paseo.dotters.network"];

struct LocalTestContext {
	#[allow(dead_code)]
	node: TestNode,
	executor: RuntimeExecutor,
	storage: LocalStorageLayer,
	metadata: Metadata,
}

struct RemoteTestContext {
	executor: RuntimeExecutor,
	storage: LocalStorageLayer,
	metadata: Metadata,
}

async fn create_local_context() -> LocalTestContext {
	let node = TestNode::spawn().await.expect("Failed to spawn test node");
	let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");
	let RemoteTestContext { executor, storage, metadata } =
		try_create_remote_context(&endpoint).await.expect("Failed to create context");

	LocalTestContext { node, executor, storage, metadata }
}

async fn try_create_remote_context(endpoint: &Url) -> Option<RemoteTestContext> {
	let rpc = ForkRpcClient::connect(endpoint).await.ok()?;
	let block_hash = rpc.finalized_head().await.ok()?;
	let header = rpc.header(block_hash).await.ok()?;
	let block_number = header.number;
	let runtime_code = rpc.runtime_code(block_hash).await.ok()?;
	let metadata = rpc.metadata(block_hash).await.ok()?;
	let cache = StorageCache::in_memory().await.ok()?;
	let remote = RemoteStorageLayer::new(rpc, cache);
	let storage = LocalStorageLayer::new(remote, block_number, block_hash, metadata.clone());
	let executor = RuntimeExecutor::new(runtime_code, None).ok()?;

	Some(RemoteTestContext { executor, storage, metadata })
}

async fn create_context_with_fallbacks(endpoints: &[&str]) -> Option<RemoteTestContext> {
	for endpoint_str in endpoints {
		let endpoint: Url = match endpoint_str.parse() {
			Ok(url) => url,
			Err(_) => continue,
		};

		if let Some(ctx) = try_create_remote_context(&endpoint).await {
			return Some(ctx);
		}
	}

	None
}

#[tokio::test(flavor = "multi_thread")]
async fn get_slot_duration_falls_back_when_aura_api_unavailable() {
	let ctx = create_local_context().await;

	let slot_duration = TimestampInherent::get_slot_duration_from_runtime(
		&ctx.executor,
		&ctx.storage,
		&ctx.metadata,
		DEFAULT_RELAY_SLOT_DURATION_MS,
	)
	.await;

	assert_eq!(
		slot_duration, DEFAULT_RELAY_SLOT_DURATION_MS,
		"Expected fallback to configured default since test node doesn't implement AuraApi or Babe"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_slot_duration_from_live_aura_chain() {
	let ctx = match create_context_with_fallbacks(ASSET_HUB_PASEO_ENDPOINTS).await {
		Some(ctx) => ctx,
		None => return,
	};

	let slot_duration = TimestampInherent::get_slot_duration_from_runtime(
		&ctx.executor,
		&ctx.storage,
		&ctx.metadata,
		0,
	)
	.await;

	assert_eq!(
		slot_duration, DEFAULT_PARA_SLOT_DURATION_MS,
		"Expected 12-second slots from Asset Hub Paseo via AuraApi"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_slot_duration_from_live_babe_chain() {
	let ctx = match create_context_with_fallbacks(PASEO_RELAY_ENDPOINTS).await {
		Some(ctx) => ctx,
		None => return,
	};

	let slot_duration = TimestampInherent::get_slot_duration_from_runtime(
		&ctx.executor,
		&ctx.storage,
		&ctx.metadata,
		0,
	)
	.await;

	assert_eq!(
		slot_duration, DEFAULT_RELAY_SLOT_DURATION_MS,
		"Expected 6-second slots from Paseo via Babe::ExpectedBlockTime"
	);
}
