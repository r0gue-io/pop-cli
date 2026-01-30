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
	/// Supported APIs.
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
	/// Block header.
	pub header: Header,
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
	#[serde(skip_serializing_if = "Option::is_none")]
	pub finalized_block_runtime: Option<RuntimeEvent>,
}

/// Runtime event for chainHead.
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodResponse {
	/// Operation result.
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
