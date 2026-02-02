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
}

/// archive_v1_storage result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum ArchiveStorageResult {
	/// Storage results returned.
	Ok {
		/// Storage result items.
		items: Vec<ArchiveStorageItem>,
	},
	/// Error occurred.
	Err {
		/// Error message.
		error: String,
	},
}

/// A single storage item result for archive_v1_storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveStorageItem {
	/// Storage key (hex-encoded).
	pub key: String,
	/// Storage value (hex-encoded), if present.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<String>,
	/// Storage hash, if requested.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hash: Option<String>,
}

/// archive_v1_call result.
///
/// Per the JSON-RPC spec:
/// - Success: `{ "success": true, "value": "0x..." }`
/// - Error: `{ "success": false, "error": "..." }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveCallResult {
	/// Whether the call succeeded.
	pub success: bool,
	/// Hex-encoded SCALE-encoded result (present on success).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<String>,
	/// Error message (present on failure).
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

impl ArchiveCallResult {
	/// Create a successful result.
	pub fn ok(value: String) -> Self {
		Self { success: true, value: Some(value), error: None }
	}

	/// Create an error result.
	pub fn err(message: String) -> Self {
		Self { success: false, value: None, error: Some(message) }
	}
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

/// Storage diff query item for archive_v1_storageDiff.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageDiffQueryItem {
	/// Storage key (hex-encoded).
	pub key: String,
	/// Return type for the diff value.
	#[serde(rename = "returnType")]
	pub return_type: StorageQueryType,
}

/// Type of storage change in a diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageDiffType {
	/// Key exists in `hash` but not in `previousHash`.
	Added,
	/// Key exists in both blocks but has different values.
	Modified,
	/// Key exists in `previousHash` but not in `hash`.
	Deleted,
}

/// A single storage diff item result for archive_v1_storageDiff.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageDiffItem {
	/// Storage key (hex-encoded).
	pub key: String,
	/// Storage value (hex-encoded), present for "added" or "modified" when returnType is "value".
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<String>,
	/// Storage hash (blake2-256), present for "added" or "modified" when returnType is "hash".
	#[serde(skip_serializing_if = "Option::is_none")]
	pub hash: Option<String>,
	/// Type of change.
	#[serde(rename = "type")]
	pub diff_type: StorageDiffType,
}

/// archive_v1_storageDiff result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum ArchiveStorageDiffResult {
	/// Storage diff results returned.
	Ok {
		/// Storage diff result items.
		items: Vec<StorageDiffItem>,
	},
	/// Error occurred.
	Err {
		/// Error message.
		error: String,
	},
}
