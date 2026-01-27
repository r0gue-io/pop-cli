// SPDX-License-Identifier: GPL-3.0

//! Common types used across RPC methods.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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

/// Block header.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
	/// Parent block hash.
	pub parent_hash: String,
	/// Block number.
	pub number: String,
	/// State root.
	pub state_root: String,
	/// Extrinsics root.
	pub extrinsics_root: String,
	/// Digest.
	pub digest: Digest,
}

/// Block digest.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Digest {
	/// Digest logs.
	pub logs: Vec<String>,
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
