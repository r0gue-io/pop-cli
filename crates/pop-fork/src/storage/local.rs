// SPDX-License-Identifier: GPL-3.0

//! Local storage layer for tracking modifications.
//!
//! This module provides the [`LocalStorageLayer`] which implements a copy-on-write
//! strategy for storage modifications. It wraps an existing [`StorageProvider`]
//! and tracks all changes (sets, deletes) in-memory without affecting the parent.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                    LocalStorageLayer                            │
//! │                                                                 │
//! │   get(key) ─────► Local Modification? ── Yes ──► Return value   │
//! │                        │                                        │
//! │                        No                                       │
//! │                        │                                        │
//! │                        ▼                                        │
//! │                  Deleted Prefix? ─────── Yes ──► Return None    │
//! │                        │                                        │
//! │                        No                                       │
//! │                        │                                        │
//! │                        ▼                                        │
//! │                  Fetch from Parent                              │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use pop_fork::{LocalStorageLayer, StorageProvider};
//!
//! let layer = LocalStorageLayer::new(parent_storage);
//!
//! // Modifications are stored locally
//! layer.set(b"key", Some(b"value"));
//!
//! // Reads prioritize local modifications
//! assert_eq!(layer.get(b"key").await?, Some(b"value".to_vec()));
//!
//! // Original parent remains unchanged
//! assert_eq!(parent_storage.get(b"key").await?, None);
//! ```

use crate::{error::RemoteStorageError, storage::StorageProvider};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};

type Data = HashMap<Vec<u8>, Option<Vec<u8>>>;

/// Local storage layer that tracks modifications on top of a parent storage provider.
///
/// It uses a copy-on-write strategy: all modifications are stored in this layer,
/// leaving the parent layer unchanged.
#[derive(Clone)]
pub struct LocalStorageLayer<P: StorageProvider + Clone> {
	parent: P,
	modifications: Arc<RwLock<Data>>,
	deleted_prefixes: Arc<RwLock<Vec<Vec<u8>>>>,
}

impl<P: StorageProvider + Clone> LocalStorageLayer<P> {
	/// Create a new layer on top of a parent.
	pub fn new(parent: P) -> Self {
		Self {
			parent,
			modifications: Arc::new(RwLock::new(HashMap::new())),
			deleted_prefixes: Arc::new(RwLock::new(Vec::new())),
		}
	}

	/// Set a storage value (local modification).
	pub fn set(&self, key: &[u8], value: Option<&[u8]>) {
		self.modifications.write().insert(key.to_vec(), value.map(|v| v.to_vec()));
	}

	/// Delete all keys with prefix.
	pub fn delete_prefix(&self, prefix: &[u8]) {
		self.deleted_prefixes.write().push(prefix.to_vec());
		// Also remove any existing modifications that match the prefix
		self.modifications.write().retain(|key, _| !key.starts_with(prefix));
	}

	/// Check if a key was explicitly deleted.
	pub fn is_deleted(&self, key: &[u8]) -> bool {
		// Check if key itself is marked as deleted (None in modifications)
		if let Some(val) = self.modifications.read().get(key) {
			return val.is_none();
		}
		// Check if key matches any deleted prefix
		let deleted_prefixes = self.deleted_prefixes.read();
		for prefix in deleted_prefixes.iter() {
			if key.starts_with(prefix) {
				return true;
			}
		}
		false
	}

	/// Get all modifications as a diff.
	pub fn diff(&self) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
		self.modifications.read().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
	}

	/// Merge another layer's modifications into this one.
	pub fn merge(&self, other: &LocalStorageLayer<impl StorageProvider + Clone>) {
		// Merge modifications
		let other_modifications = other.modifications.read();
		let mut modifications = self.modifications.write();
		for (key, value) in other_modifications.iter() {
			modifications.insert(key.clone(), value.clone());
		}

		// Merge deleted prefixes
		let other_prefixes = other.deleted_prefixes.read();
		let mut deleted_prefixes = self.deleted_prefixes.write();
		for prefix in other_prefixes.iter() {
			if !deleted_prefixes.contains(prefix) {
				deleted_prefixes.push(prefix.clone());
				// Remove existing modifications that match the new prefix
				modifications.retain(|key, _| !key.starts_with(prefix));
			}
		}
	}

	/// Create a child layer for nested modifications.
	pub fn child(&self) -> LocalStorageLayer<Self> {
		LocalStorageLayer::new(self.clone())
	}
}

#[async_trait]
impl<P: StorageProvider + Clone> StorageProvider for LocalStorageLayer<P> {
	async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, RemoteStorageError> {
		// 1. Check local modifications
		{
			let modifications = self.modifications.read();
			if let Some(value) = modifications.get(key) {
				return Ok(value.clone());
			}
		}

		// 2. Check if key matches a deleted prefix
		{
			let deleted_prefixes = self.deleted_prefixes.read();
			for prefix in deleted_prefixes.iter() {
				if key.starts_with(prefix) {
					return Ok(None);
				}
			}
		}

		// 3. Delegate to parent.get(key)
		self.parent.get(key).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[derive(Clone)]
	struct MockParent {
		data: HashMap<Vec<u8>, Vec<u8>>,
	}

	#[async_trait]
	impl StorageProvider for MockParent {
		async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, RemoteStorageError> {
			Ok(self.data.get(key).cloned())
		}
	}

	#[tokio::test]
	async fn test_local_storage_layer_get_set() {
		let mut data = HashMap::new();
		data.insert(b"key1".to_vec(), b"val1".to_vec());
		let parent = MockParent { data };
		let layer = LocalStorageLayer::new(parent);

		// Get from parent
		assert_eq!(layer.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
		assert_eq!(layer.get(b"key2").await.unwrap(), None);

		// Set locally
		layer.set(b"key2", Some(b"val2"));
		assert_eq!(layer.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));

		// Override parent
		layer.set(b"key1", Some(b"val1_new"));
		assert_eq!(layer.get(b"key1").await.unwrap(), Some(b"val1_new".to_vec()));

		// Delete locally
		layer.set(b"key1", None);
		assert_eq!(layer.get(b"key1").await.unwrap(), None);
	}

	#[tokio::test]
	async fn test_delete_prefix() {
		let mut data = HashMap::new();
		data.insert(b"abc1".to_vec(), b"val1".to_vec());
		data.insert(b"abc2".to_vec(), b"val2".to_vec());
		data.insert(b"def1".to_vec(), b"val3".to_vec());
		let parent = MockParent { data };
		let layer = LocalStorageLayer::new(parent);

		layer.delete_prefix(b"abc");
		assert_eq!(layer.get(b"abc1").await.unwrap(), None);
		assert_eq!(layer.get(b"abc2").await.unwrap(), None);
		assert_eq!(layer.get(b"def1").await.unwrap(), Some(b"val3".to_vec()));

		// Check is_deleted
		assert!(layer.is_deleted(b"abc1"));
		assert!(layer.is_deleted(b"abc_new"));
		assert!(!layer.is_deleted(b"def1"));
	}

	#[tokio::test]
	async fn test_diff_and_merge() {
		let parent = MockParent { data: HashMap::new() };
		let layer1 = LocalStorageLayer::new(parent.clone());
		let layer2 = LocalStorageLayer::new(parent);

		layer1.set(b"key1", Some(b"val1"));
		layer2.set(b"key2", Some(b"val2"));
		layer2.delete_prefix(b"pfx");

		layer1.merge(&layer2);
		assert_eq!(layer1.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));
		assert_eq!(layer1.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));
		assert!(layer1.is_deleted(b"pfx_anything"));

		let diff = layer1.diff();
		assert_eq!(diff.len(), 2);
	}

	#[tokio::test]
	async fn test_nested_layers() {
		let mut data = HashMap::new();
		data.insert(b"key1".to_vec(), b"val1".to_vec());
		let parent = MockParent { data };
		let layer1 = LocalStorageLayer::new(parent);
		let layer2 = layer1.child();

		layer2.set(b"key1", Some(b"val1_v2"));
		assert_eq!(layer2.get(b"key1").await.unwrap(), Some(b"val1_v2".to_vec()));
		assert_eq!(layer1.get(b"key1").await.unwrap(), Some(b"val1".to_vec()));

		layer1.set(b"key2", Some(b"val2"));
		// layer2 should see layer1 changes if it hasn't overridden them
		assert_eq!(layer2.get(b"key2").await.unwrap(), Some(b"val2".to_vec()));
	}
}
