// SPDX-License-Identifier: GPL-3.0

//! Common types used across RPC methods.

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};

/// A hex-encoded string with "0x" prefix.
///
/// This type handles the common pattern of encoding/decoding hex strings
/// for RPC communication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HexString(String);

impl HexString {
	/// Create a new HexString from raw bytes.
	pub fn from_bytes(bytes: &[u8]) -> Self {
		Self(format!("0x{}", hex::encode(bytes)))
	}

	/// Decode the hex string back to raw bytes.
	pub fn to_bytes(&self) -> Result<Vec<u8>, hex::FromHexError> {
		hex::decode(self.0.trim_start_matches("0x"))
	}

	/// Get the inner string representation.
	pub fn as_str(&self) -> &str {
		&self.0
	}

	/// Convert into the inner String.
	pub fn into_inner(self) -> String {
		self.0
	}
}

impl fmt::Display for HexString {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl From<HexString> for String {
	fn from(hex: HexString) -> Self {
		hex.0
	}
}

impl From<&[u8]> for HexString {
	fn from(bytes: &[u8]) -> Self {
		Self::from_bytes(bytes)
	}
}

impl<const N: usize> From<&[u8; N]> for HexString {
	fn from(bytes: &[u8; N]) -> Self {
		Self::from_bytes(bytes)
	}
}

/// Subxt's built-in header type for decoding SCALE-encoded headers.
pub type Header = <subxt::SubstrateConfig as subxt::Config>::Header;

/// RPC-compatible header format for polkadot.js.
///
/// This matches the format expected by polkadot.js apps:
/// - Block number as hex string with 0x prefix
/// - Hashes as hex strings with 0x prefix
/// - Digest logs as hex strings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcHeader {
	/// Parent block hash.
	pub parent_hash: String,
	/// Block number as hex string (e.g., "0x3039").
	pub number: String,
	/// State root hash.
	pub state_root: String,
	/// Extrinsics root hash.
	pub extrinsics_root: String,
	/// Block digest.
	pub digest: RpcDigest,
}

/// RPC-compatible digest format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcDigest {
	/// Digest logs as hex strings.
	pub logs: Vec<String>,
}

impl RpcHeader {
	/// Convert from subxt's Header type to RPC-compatible format.
	pub fn from_header(header: &Header) -> Self {
		use scale::Encode;
		Self {
			parent_hash: HexString::from_bytes(header.parent_hash.as_bytes()).into(),
			number: format!("0x{:x}", header.number),
			state_root: HexString::from_bytes(header.state_root.as_bytes()).into(),
			extrinsics_root: HexString::from_bytes(header.extrinsics_root.as_bytes()).into(),
			digest: RpcDigest {
				logs: header
					.digest
					.logs
					.iter()
					.map(|log| HexString::from_bytes(&log.encode()).into())
					.collect(),
			},
		}
	}
}

/// Runtime version information.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeVersion {
	/// Spec name (e.g., "polkadot").
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
	/// State version.
	pub state_version: u8,
	/// Supported APIs as array of [api_id, version] tuples.
	/// polkadot-js expects: [["0xdf6acb689907609b", 4], ...]
	#[serde(default)]
	pub apis: Vec<(String, u32)>,
}

/// System health status.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemHealth {
	/// Number of connected peers.
	pub peers: u32,
	/// Is the node syncing?
	pub is_syncing: bool,
	/// Should this node have any peers?
	pub should_have_peers: bool,
}

/// Chain properties.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChainProperties {
	/// Token decimals.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub token_decimals: Option<u32>,

	/// Token symbol.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub token_symbol: Option<String>,

	/// SS58 address format.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub ss58_format: Option<u16>,

	/// Additional properties.
	#[serde(flatten)]
	pub additional: HashMap<String, serde_json::Value>,
}

/// Signed block (header + extrinsics).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedBlock {
	/// Block data.
	pub block: BlockData,
	/// Justifications (if any).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub justifications: Option<Vec<Vec<u8>>>,
}

/// Block data (header + extrinsics).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockData {
	/// Block header in RPC-compatible format.
	pub header: RpcHeader,
	/// Extrinsics.
	pub extrinsics: Vec<String>,
}

/// chainHead event types for subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum ChainHeadEvent {
	/// Subscription initialized.
	Initialized(InitializedEvent),
	/// New block announced.
	NewBlock(NewBlockEvent),
	/// Best block changed.
	BestBlockChanged(BestBlockChangedEvent),
	/// Block finalized.
	Finalized(FinalizedEvent),
	/// Subscription stopped.
	Stop,
}

/// Initialized event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializedEvent {
	/// Finalized block hashes.
	pub finalized_block_hashes: Vec<String>,
	/// Finalized block runtime (if requested).
	/// This is a flat runtime object for papi compatibility (not wrapped in ValidRuntime).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub finalized_block_runtime: Option<ChainHeadRuntimeVersion>,
}

/// Runtime version for chainHead RPC methods.
///
/// This differs from `RuntimeVersion` in that:
/// - It's a flat object (not wrapped in `{ type: "valid", spec: {...} }`)
/// - The `apis` field is a HashMap (JSON object) instead of Vec (JSON array)
///
/// papi-console expects: `{ specName, implName, ..., apis: { "0x...": 1 } }`
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChainHeadRuntimeVersion {
	/// Spec name (e.g., "polkadot").
	pub spec_name: String,
	/// Implementation name.
	pub impl_name: String,
	/// Spec version.
	pub spec_version: u32,
	/// Implementation version.
	pub impl_version: u32,
	/// Transaction version.
	pub transaction_version: u32,
	/// Supported APIs as a map from hex-encoded API ID to version.
	#[serde(default)]
	pub apis: HashMap<String, u32>,
}

/// Runtime event for chainHead (kept for newBlock events which may need it).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuntimeEvent {
	/// Valid runtime.
	Valid(ValidRuntime),
	/// Invalid runtime.
	Invalid { error: String },
}

/// Valid runtime info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidRuntime {
	/// Runtime specification.
	pub spec: RuntimeVersion,
}

/// New block event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBlockEvent {
	/// Block hash.
	pub block_hash: String,
	/// Parent block hash.
	pub parent_block_hash: String,
	/// New runtime (if changed).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub new_runtime: Option<RuntimeEvent>,
}

/// Best block changed event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BestBlockChangedEvent {
	/// Best block hash.
	pub best_block_hash: String,
}

/// Finalized event data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizedEvent {
	/// Finalized block hashes.
	pub finalized_block_hashes: Vec<String>,
	/// Pruned block hashes.
	pub pruned_block_hashes: Vec<String>,
}

/// Operation response for chainHead methods.
///
/// Note: Uses `#[serde(flatten)]` to avoid double nesting of "result".
/// The spec expects `{"result": "started", "operationId": "..."}` not
/// `{"result": {"result": "started", ...}}`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodResponse {
	/// Operation result.
	#[serde(flatten)]
	pub result: OperationResult,
}

/// Operation result variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum OperationResult {
	/// Operation started.
	Started { operation_id: String },
	/// Limit reached.
	LimitReached,
}

/// Storage change set for subscriptions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageChangeSet {
	/// Block hash where changes occurred.
	pub block: String,
	/// List of storage changes (key, value).
	pub changes: Vec<(String, Option<String>)>,
}

/// Operation event for chainHead subscription.
///
/// These events are sent via the chainHead_v1_followEvent subscription
/// to report the results of async operations (body, call, storage).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum OperationEvent {
	/// Body operation completed successfully.
	#[serde(rename_all = "camelCase")]
	OperationBodyDone {
		/// Operation identifier.
		operation_id: String,
		/// Hex-encoded extrinsics.
		value: Vec<String>,
	},
	/// Call operation completed successfully.
	#[serde(rename_all = "camelCase")]
	OperationCallDone {
		/// Operation identifier.
		operation_id: String,
		/// Hex-encoded call output.
		output: String,
	},
	/// Storage items returned.
	#[serde(rename_all = "camelCase")]
	OperationStorageItems {
		/// Operation identifier.
		operation_id: String,
		/// Storage items.
		items: Vec<StorageResultItem>,
	},
	/// Storage operation completed.
	#[serde(rename_all = "camelCase")]
	OperationStorageDone {
		/// Operation identifier.
		operation_id: String,
	},
	/// Operation failed with error.
	#[serde(rename_all = "camelCase")]
	OperationError {
		/// Operation identifier.
		operation_id: String,
		/// Error message.
		error: String,
	},
}

/// Storage result item for chainHead_v1_storage responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageResultItem {
	/// Storage key (hex-encoded).
	pub key: String,
	/// Storage value (hex-encoded, if exists).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<String>,
	/// Hash of the value (for hash queries).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hash: Option<String>,
}

/// System sync state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncState {
	/// Starting block number.
	pub starting_block: u32,
	/// Current block number.
	pub current_block: u32,
	/// Highest known block number.
	pub highest_block: u32,
}
