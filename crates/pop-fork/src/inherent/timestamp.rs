// SPDX-License-Identifier: GPL-3.0

//! Timestamp inherent provider for block building.
//!
//! This module provides [`TimestampInherent`] which generates the mandatory
//! timestamp inherent extrinsic for each block. The timestamp pallet requires
//! this inherent to advance the chain's notion of time.
//!
//! # How It Works
//!
//! 1. Look up the Timestamp pallet and `set` call indices from runtime metadata
//! 2. Detect slot duration using the following fallback chain:
//!    - `AuraApi_slot_duration` runtime API (Aura-based chains)
//!    - `Babe::ExpectedBlockTime` metadata constant (Babe-based chains)
//!    - Configured default slot duration
//! 3. Read the current timestamp from `Timestamp::Now` storage
//! 4. Add the slot duration to get the new timestamp
//! 5. Encode a `timestamp.set(new_timestamp)` call using the dynamic indices
//! 6. Wrap it as an unsigned inherent extrinsic
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::inherent::TimestampInherent;
//!
//! // Create with default 6-second slots (relay chain)
//! let provider = TimestampInherent::default_relay();
//!
//! // Create with custom slot duration
//! let provider = TimestampInherent::new(2_000); // 2 seconds
//! ```

use crate::{
	Block, BlockBuilderError, RuntimeExecutor, inherent::InherentProvider,
	strings::inherent::timestamp as strings,
};
use async_trait::async_trait;
use scale::{Compact, Decode, Encode};
use subxt::Metadata;

/// Default slot duration for relay chains (6 seconds).
const DEFAULT_RELAY_SLOT_DURATION_MS: u64 = 6_000;

/// Default slot duration for parachains (12 seconds).
const DEFAULT_PARA_SLOT_DURATION_MS: u64 = 12_000;

/// Extrinsic format byte for bare/unsigned extrinsics.
///
/// Both v4 and v5 extrinsic formats use 0x04 for bare extrinsics:
/// - V4: version byte = 0x04 (unsigned, no signature)
/// - V5: mode byte = 0x04 (bare extrinsic, no extensions)
const BARE_EXTRINSIC_VERSION: u8 = 0x04;

/// Timestamp inherent provider.
///
/// Generates the `timestamp.set(now)` inherent extrinsic that advances
/// the chain's timestamp by the slot duration.
///
/// # Slot Duration Detection
///
/// The provider attempts to detect the slot duration dynamically using
/// the following fallback chain:
/// 1. `AuraApi_slot_duration` runtime API (Aura-based chains)
/// 2. `Babe::ExpectedBlockTime` metadata constant (Babe-based chains)
/// 3. Configured default slot duration (default: 6 seconds)
///
/// # Dynamic Metadata Lookup
///
/// The pallet and call indices are looked up dynamically from the runtime
/// metadata, making this provider work across different runtimes without
/// manual configuration.
#[derive(Debug, Clone)]
pub struct TimestampInherent {
	/// Slot duration in milliseconds.
	slot_duration_ms: u64,
}

impl TimestampInherent {
	/// Create a new timestamp inherent provider.
	///
	/// # Arguments
	///
	/// * `slot_duration_ms` - Slot duration in milliseconds
	pub fn new(slot_duration_ms: u64) -> Self {
		Self { slot_duration_ms }
	}

	/// Create with default settings for relay chains (6-second slots).
	pub fn default_relay() -> Self {
		Self::new(DEFAULT_RELAY_SLOT_DURATION_MS)
	}

	/// Create with default settings for parachains (12-second slots).
	pub fn default_para() -> Self {
		Self::new(DEFAULT_PARA_SLOT_DURATION_MS)
	}

	/// Compute the storage key for `Timestamp::Now`.
	fn timestamp_now_key() -> Vec<u8> {
		let pallet_hash = sp_core::twox_128(strings::storage_keys::PALLET_NAME);
		let storage_hash = sp_core::twox_128(strings::storage_keys::NOW);
		[pallet_hash.as_slice(), storage_hash.as_slice()].concat()
	}

	/// Encode the `timestamp.set(now)` call.
	///
	/// The call is encoded as: `[pallet_index, call_index, Compact<u64>]`
	fn encode_timestamp_set_call(pallet_index: u8, call_index: u8, timestamp: u64) -> Vec<u8> {
		let mut call = vec![pallet_index, call_index];
		// Timestamp argument is encoded as Compact<u64>
		call.extend(Compact(timestamp).encode());
		call
	}

	/// Wrap a call as a bare inherent extrinsic.
	///
	/// Bare extrinsics have the format:
	/// - Compact length prefix
	/// - Version/mode byte (0x04 for bare)
	/// - Call data
	fn encode_inherent_extrinsic(call: Vec<u8>) -> Vec<u8> {
		let mut extrinsic = vec![BARE_EXTRINSIC_VERSION];
		extrinsic.extend(call);

		// Prefix with compact length
		let len = Compact(extrinsic.len() as u32);
		let mut result = len.encode();
		result.extend(extrinsic);
		result
	}

	/// Get slot duration from the runtime, falling back to the provided default.
	///
	/// Detection order:
	/// 1. `AuraApi_slot_duration` runtime API (Aura-based chains)
	/// 2. `Babe::ExpectedBlockTime` metadata constant (Babe-based chains)
	/// 3. Fallback to configured default
	async fn get_slot_duration_from_runtime(
		executor: &RuntimeExecutor,
		storage: &crate::LocalStorageLayer,
		metadata: &Metadata,
		fallback: u64,
	) -> u64 {
		// 1. Try AuraApi_slot_duration runtime API
		if let Some(duration) = executor
			.call(strings::slot_duration::AURA_API_METHOD, &[], storage)
			.await
			.ok()
			.and_then(|r| u64::decode(&mut r.output.as_slice()).ok())
		{
			return duration;
		}

		// 2. Try Babe::ExpectedBlockTime metadata constant
		if let Some(duration) = Self::get_constant_from_metadata(
			metadata,
			strings::slot_duration::BABE_PALLET,
			strings::slot_duration::BABE_EXPECTED_BLOCK_TIME,
		) {
			return duration;
		}

		// 3. Fall back to configured default
		fallback
	}

	/// Attempt to read a u64 constant from metadata.
	fn get_constant_from_metadata(
		metadata: &Metadata,
		pallet_name: &str,
		constant_name: &str,
	) -> Option<u64> {
		metadata
			.pallet_by_name(pallet_name)?
			.constant_by_name(constant_name)
			.and_then(|c| u64::decode(&mut &c.value()[..]).ok())
	}
}

impl Default for TimestampInherent {
	fn default() -> Self {
		Self::default_relay()
	}
}

#[async_trait]
impl InherentProvider for TimestampInherent {
	fn identifier(&self) -> &'static str {
		strings::IDENTIFIER
	}

	async fn provide(
		&self,
		parent: &Block,
		executor: &RuntimeExecutor,
	) -> Result<Vec<Vec<u8>>, BlockBuilderError> {
		// Look up pallet and call indices from metadata
		let metadata = parent.metadata().await?;

		let pallet = metadata.pallet_by_name(strings::metadata::PALLET_NAME).ok_or_else(|| {
			BlockBuilderError::InherentProvider {
				provider: self.identifier().to_string(),
				message: format!(
					"{}: {}",
					strings::errors::PALLET_NOT_FOUND,
					strings::metadata::PALLET_NAME
				),
			}
		})?;

		let pallet_index = pallet.index();

		let call_variant = pallet
			.call_variant_by_name(strings::metadata::SET_CALL_NAME)
			.ok_or_else(|| BlockBuilderError::InherentProvider {
				provider: self.identifier().to_string(),
				message: format!(
					"{}: {}",
					strings::errors::CALL_NOT_FOUND,
					strings::metadata::SET_CALL_NAME
				),
			})?;

		let call_index = call_variant.index;

		// Get slot duration: try runtime API/constants, fall back to configured value
		let storage = parent.storage();
		let slot_duration = Self::get_slot_duration_from_runtime(
			executor,
			storage,
			&metadata,
			self.slot_duration_ms,
		)
		.await;

		// Read current timestamp from parent block storage
		let key = Self::timestamp_now_key();

		let current_timestamp = match storage.get(parent.number, &key).await? {
			Some(value) if value.value.is_some() => u64::decode(
				&mut value
					.value
					.as_ref()
					.expect("The match guard ensures it's Some; qed;")
					.as_slice(),
			)
			.map_err(|e| BlockBuilderError::InherentProvider {
				provider: self.identifier().to_string(),
				message: format!("{}: {}", strings::errors::DECODE_FAILED, e),
			})?,
			_ => {
				// No timestamp set yet (genesis or very early block)
				// Use current system time as fallback
				std::time::SystemTime::now()
					.duration_since(std::time::UNIX_EPOCH)
					.map(|d| d.as_millis() as u64)
					.unwrap_or(0)
			},
		};

		// Calculate new timestamp
		let new_timestamp = current_timestamp.saturating_add(slot_duration);

		eprintln!(
			"[Timestamp] current_timestamp={current_timestamp}, slot_duration={slot_duration}, new_timestamp={new_timestamp}"
		);
		eprintln!("[Timestamp] pallet_index={pallet_index}, call_index={call_index}");

		// Encode the timestamp.set call with dynamic indices
		let call = Self::encode_timestamp_set_call(pallet_index, call_index, new_timestamp);

		// Wrap as unsigned extrinsic
		let extrinsic = Self::encode_inherent_extrinsic(call);

		eprintln!(
			"[Timestamp] extrinsic ({} bytes): 0x{}",
			extrinsic.len(),
			hex::encode(&extrinsic)
		);

		Ok(vec![extrinsic])
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Custom slot duration for testing (1 second).
	const TEST_SLOT_DURATION_MS: u64 = 1_000;

	/// Test pallet index (arbitrary value for encoding tests).
	const TEST_PALLET_INDEX: u8 = 3;

	/// Test call index (arbitrary value for encoding tests).
	const TEST_CALL_INDEX: u8 = 0;

	#[test]
	fn new_creates_provider_with_slot_duration() {
		let provider = TimestampInherent::new(TEST_SLOT_DURATION_MS);
		assert_eq!(provider.slot_duration_ms, TEST_SLOT_DURATION_MS);
	}

	#[test]
	fn default_relay_uses_configured_slot_duration() {
		let provider = TimestampInherent::default_relay();
		assert_eq!(provider.slot_duration_ms, DEFAULT_RELAY_SLOT_DURATION_MS);
	}

	#[test]
	fn default_para_uses_configured_slot_duration() {
		let provider = TimestampInherent::default_para();
		assert_eq!(provider.slot_duration_ms, DEFAULT_PARA_SLOT_DURATION_MS);
	}

	#[test]
	fn timestamp_now_key_is_32_bytes() {
		let key = TimestampInherent::timestamp_now_key();
		// twox128 produces 16 bytes per hash, storage key = pallet hash + item hash
		const TWOX128_OUTPUT_BYTES: usize = 16;
		const STORAGE_KEY_LEN: usize = TWOX128_OUTPUT_BYTES * 2;
		assert_eq!(key.len(), STORAGE_KEY_LEN);
	}

	#[test]
	fn encode_timestamp_set_call_produces_valid_encoding() {
		let call = TimestampInherent::encode_timestamp_set_call(
			TEST_PALLET_INDEX,
			TEST_CALL_INDEX,
			1_000_000,
		);

		// First byte is pallet index
		assert_eq!(call[0], TEST_PALLET_INDEX);
		// Second byte is call index
		assert_eq!(call[1], TEST_CALL_INDEX);
		// Rest is compact-encoded timestamp
		assert!(call.len() > 2);
	}

	#[test]
	fn encode_inherent_extrinsic_includes_version_and_length() {
		// Create fake call data
		let call = vec![TEST_PALLET_INDEX, TEST_CALL_INDEX, 1, 2, 3];
		let extrinsic = TimestampInherent::encode_inherent_extrinsic(call.clone());

		// Should start with compact length (6 = version byte + 5 call bytes)
		// Compact encoding of 6 is (6 << 2) = 0x18
		const EXPECTED_COMPACT_LEN: u8 = 0x18;
		assert_eq!(extrinsic[0], EXPECTED_COMPACT_LEN);
		// Next byte is bare extrinsic version (0x04)
		assert_eq!(extrinsic[1], BARE_EXTRINSIC_VERSION);
		// Rest is the call
		assert_eq!(&extrinsic[2..], &call[..]);
	}

	#[test]
	fn identifier_returns_timestamp() {
		let provider = TimestampInherent::default();
		assert_eq!(provider.identifier(), strings::IDENTIFIER);
	}

	/// Integration tests that execute against a local test node.
	///
	/// These tests verify the runtime API call for slot duration detection.
	mod sequential {
		use super::*;
		use crate::{
			ForkRpcClient, LocalStorageLayer, RemoteStorageLayer, RuntimeExecutor, StorageCache,
		};
		use pop_common::test_env::TestNode;
		use url::Url;

		/// Asset Hub Paseo endpoints (Aura-based parachain).
		/// Multiple endpoints for redundancy in CI.
		const ASSET_HUB_PASEO_ENDPOINTS: &[&str] = &[
			"wss://sys.ibp.network/asset-hub-paseo",
			"wss://sys.turboflakes.io/asset-hub-paseo",
			"wss://asset-hub-paseo.dotters.network",
		];

		/// Paseo relay chain endpoints (Babe-based chain).
		/// Multiple endpoints for redundancy in CI.
		const PASEO_RELAY_ENDPOINTS: &[&str] = &[
			"wss://rpc.ibp.network/paseo",
			"wss://pas-rpc.stakeworld.io",
			"wss://paseo.dotters.network",
		];

		/// Test context for slot duration tests with a local test node.
		struct LocalTestContext {
			#[allow(dead_code)]
			node: TestNode,
			executor: RuntimeExecutor,
			storage: LocalStorageLayer,
			metadata: Metadata,
		}

		/// Test context for slot duration tests with a remote endpoint.
		struct RemoteTestContext {
			executor: RuntimeExecutor,
			storage: LocalStorageLayer,
			metadata: Metadata,
		}

		/// Creates a test context from a local test node.
		async fn create_local_context() -> LocalTestContext {
			let node = TestNode::spawn().await.expect("Failed to spawn test node");
			let endpoint: Url = node.ws_url().parse().expect("Invalid WebSocket URL");
			let RemoteTestContext { executor, storage, metadata } =
				try_create_remote_context(&endpoint).await.expect("Failed to create context");

			LocalTestContext { node, executor, storage, metadata }
		}

		/// Attempts to create a test context from a remote endpoint URL.
		/// Returns None if connection fails, allowing callers to try alternative endpoints.
		async fn try_create_remote_context(endpoint: &Url) -> Option<RemoteTestContext> {
			let rpc = ForkRpcClient::connect(endpoint).await.ok()?;
			let block_hash = rpc.finalized_head().await.ok()?;
			let header = rpc.header(block_hash).await.ok()?;
			let block_number = header.number;
			let runtime_code = rpc.runtime_code(block_hash).await.ok()?;
			let metadata_bytes = rpc.metadata(block_hash).await.ok()?;
			let metadata = Metadata::decode(&mut metadata_bytes.as_slice()).ok()?;
			let cache = StorageCache::in_memory().await.ok()?;
			let remote = RemoteStorageLayer::new(rpc, cache);
			let storage =
				LocalStorageLayer::new(remote, block_number, block_hash, metadata.clone());
			let executor = RuntimeExecutor::new(runtime_code, None).ok()?;

			Some(RemoteTestContext { executor, storage, metadata })
		}

		/// Creates a test context by trying multiple endpoints in sequence.
		/// Returns the first successful connection, or None if all fail.
		async fn create_context_with_fallbacks(endpoints: &[&str]) -> Option<RemoteTestContext> {
			for endpoint_str in endpoints {
				let endpoint: Url = match endpoint_str.parse() {
					Ok(url) => url,
					Err(_) => continue,
				};

				println!("Trying endpoint: {endpoint_str}");

				if let Some(ctx) = try_create_remote_context(&endpoint).await {
					println!("Connected to: {endpoint_str}");
					return Some(ctx);
				}

				println!("Failed to connect to: {endpoint_str}");
			}

			None
		}

		/// Tests that slot duration detection falls back to configured default when
		/// the runtime doesn't expose `AuraApi_slot_duration`.
		///
		/// The test node uses manual seal consensus, not Aura,
		/// so the runtime API call fails and returns the fallback value directly.
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

			// TestNode doesn't implement AuraApi or Babe constants,
			// so we get the fallback value directly.
			println!("Slot duration (with fallback): {slot_duration}ms");
			assert_eq!(
				slot_duration, DEFAULT_RELAY_SLOT_DURATION_MS,
				"Expected fallback to configured default since test node doesn't implement AuraApi or Babe"
			);
		}

		/// Tests that slot duration can be fetched from a live Aura-based chain.
		///
		/// This test connects to Asset Hub Paseo (an Aura-based parachain) and
		/// verifies that `AuraApi_slot_duration` returns the expected 12-second slots.
		///
		/// Multiple endpoints are tried for redundancy in CI environments.
		#[tokio::test(flavor = "multi_thread")]
		async fn get_slot_duration_from_live_aura_chain() {
			const EXPECTED_SLOT_DURATION_MS: u64 = DEFAULT_PARA_SLOT_DURATION_MS;

			let ctx = match create_context_with_fallbacks(ASSET_HUB_PASEO_ENDPOINTS).await {
				Some(ctx) => ctx,
				None => {
					eprintln!(
						"Skipping test: all Asset Hub Paseo endpoints unavailable: {:?}",
						ASSET_HUB_PASEO_ENDPOINTS
					);
					return;
				},
			};

			let slot_duration = TimestampInherent::get_slot_duration_from_runtime(
				&ctx.executor,
				&ctx.storage,
				&ctx.metadata,
				0, // fallback won't be used
			)
			.await;

			println!("Asset Hub Paseo - slot duration: {slot_duration}ms");

			assert_eq!(
				slot_duration, EXPECTED_SLOT_DURATION_MS,
				"Expected 12-second slots from Asset Hub Paseo via AuraApi"
			);
		}

		/// Tests that slot duration is fetched from Babe::ExpectedBlockTime on a live Babe-based
		/// chain.
		///
		/// This test connects to Paseo relay chain (a Babe-based chain) and
		/// verifies that the slot duration is read from the `Babe::ExpectedBlockTime`
		/// metadata constant, returning the expected 6-second slots.
		///
		/// Multiple endpoints are tried for redundancy in CI environments.
		#[tokio::test(flavor = "multi_thread")]
		async fn get_slot_duration_from_live_babe_chain() {
			const EXPECTED_SLOT_DURATION_MS: u64 = DEFAULT_RELAY_SLOT_DURATION_MS;

			let ctx = match create_context_with_fallbacks(PASEO_RELAY_ENDPOINTS).await {
				Some(ctx) => ctx,
				None => {
					eprintln!(
						"Skipping test: all Paseo relay endpoints unavailable: {:?}",
						PASEO_RELAY_ENDPOINTS
					);
					return;
				},
			};

			let slot_duration = TimestampInherent::get_slot_duration_from_runtime(
				&ctx.executor,
				&ctx.storage,
				&ctx.metadata,
				0, // fallback won't be used
			)
			.await;

			println!("Paseo (Babe) - slot duration: {slot_duration}ms");

			assert_eq!(
				slot_duration, EXPECTED_SLOT_DURATION_MS,
				"Expected 6-second slots from Paseo via Babe::ExpectedBlockTime"
			);
		}
	}
}
