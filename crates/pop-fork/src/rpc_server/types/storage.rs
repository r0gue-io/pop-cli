// SPDX-License-Identifier: GPL-3.0

//! Storage-related RPC types.

use serde::{Deserialize, Serialize};

/// Storage query item for chainHead_v1_storage and archive_v1_storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageQueryItem {
	/// Storage key (hex-encoded).
	pub key: String,
	/// Query type.
	#[serde(rename = "type")]
	pub query_type: StorageQueryType,
}

/// Storage query type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageQueryType {
	/// Get value.
	Value,
	/// Get hash of value.
	Hash,
	/// Get closest descendant merkle value.
	ClosestDescendantMerkleValue,
	/// Get descendants' values.
	DescendantsValues,
	/// Get descendants' hashes.
	DescendantsHashes,
}

/// Storage result item.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageResultItem {
	/// Storage key (hex-encoded).
	pub key: String,
	/// Result based on query type.
	#[serde(flatten)]
	pub result: StorageResultValue,
}

/// Storage result value variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageResultValue {
	/// Value result.
	Value(String),
	/// Hash result.
	Hash(String),
	/// Closest descendant merkle value.
	ClosestDescendantMerkleValue(String),
}

/// archive_v1_storage result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum ArchiveStorageResult {
	/// Storage operation started.
	Ok {
		/// Operation ID for tracking.
		#[serde(skip_serializing_if = "Option::is_none")]
		operation_id: Option<String>,
	},
	/// Error occurred.
	Err {
		/// Error message.
		error: String,
	},
}

/// archive_v1_call result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum ArchiveCallResult {
	/// Call succeeded.
	Ok {
		/// Hex-encoded result.
		output: String,
	},
	/// Call failed.
	Err {
		/// Error message.
		error: String,
	},
}

/// archive_v1_hashByHeight result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HashByHeightResult {
	/// Block hashes at the height.
	Hashes(Vec<String>),
	/// No blocks at this height.
	None,
}

/// Hash or array of hashes for chainHead_v1_unpin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HashOrHashes {
	/// Single hash.
	Single(String),
	/// Multiple hashes.
	Multiple(Vec<String>),
}
