// SPDX-License-Identifier: GPL-3.0

use crate::{error::LocalStorageError, remote::RemoteStorageLayer};
use std::{
	collections::HashMap,
	sync::{Arc, RwLock},
};

type SharedValue = Arc<Vec<u8>>;
type Modifications = HashMap<Vec<u8>, Option<SharedValue>>;
type DeletedPrefixes = Vec<Vec<u8>>;

#[derive(Clone, Debug)]
pub struct LocalStorageLayer {
	parent: RemoteStorageLayer,
	modifications: Arc<RwLock<Modifications>>,
	deleted_prefixes: Arc<RwLock<DeletedPrefixes>>,
}

impl LocalStorageLayer {
	pub fn new(parent: RemoteStorageLayer) -> Self {
		Self {
			parent,
			modifications: Arc::new(RwLock::new(HashMap::new())),
			deleted_prefixes: Arc::new(RwLock::new(Vec::new())),
		}
	}

	pub async fn get(&self, key: &[u8]) -> Result<Option<SharedValue>, LocalStorageError> {
		let modifications_lock = self
			.modifications
			.try_read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let deleted_prefixes_lock = self
			.deleted_prefixes
			.try_read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		match modifications_lock.get(key) {
			Some(value) => Ok(value.clone()),
			None if deleted_prefixes_lock.iter().any(|prefix| prefix.as_slice() == key) => Ok(None),
			_ => Ok(self.parent.get(key).await?.map(|value| Arc::new(value))),
		}
	}

	pub fn set(&self, key: &[u8], value: Option<&[u8]>) -> Result<(), LocalStorageError> {
		let mut modifications_lock = self
			.modifications
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		modifications_lock.insert(key.to_vec(), value.map(|value| Arc::new(value.to_vec())));

		Ok(())
	}

	pub fn delete_prefix(&self, prefix: &[u8]) -> Result<(), LocalStorageError> {
		let mut modifications_lock = self
			.modifications
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let mut deleted_prefixes_lock = self
			.deleted_prefixes
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		// Remove all keys starting with the prefix using retain
		modifications_lock.retain(|key, _| !key.starts_with(prefix));

		// Add prefix to deleted_prefixes
		deleted_prefixes_lock.push(prefix.to_vec());

		Ok(())
	}

	pub fn is_deleted(&self, prefix: &[u8]) -> Result<bool, LocalStorageError> {
		let deleted_prefixes_lock = self
			.deleted_prefixes
			.try_read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		Ok(deleted_prefixes_lock
			.iter()
			.any(|deleted_prefix| deleted_prefix.as_slice() == prefix))
	}

	pub fn diff(&self) -> Result<Vec<(Vec<u8>, Option<SharedValue>)>, LocalStorageError> {
		let modifications_lock = self
			.modifications
			.try_read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		Ok(modifications_lock
			.iter()
			.map(|(key, value)| (key.clone(), value.clone()))
			.collect())
	}

	pub fn merge(&self, other: &LocalStorageLayer) -> Result<(), LocalStorageError> {
		let mut self_modifications = self
			.modifications
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let other_modifications = other
			.modifications
			.try_read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let mut self_deleted_prefixes = self
			.deleted_prefixes
			.try_write()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;
		let other_deleted_prefixes = other
			.deleted_prefixes
			.try_read()
			.map_err(|e| LocalStorageError::Lock(e.to_string()))?;

		// Merge modifications (other's values take precedence)
		for (key, value) in other_modifications.iter() {
			self_modifications.insert(key.clone(), value.clone());
		}

		// Extend deleted prefixes, avoiding duplicates
		for prefix in other_deleted_prefixes.iter() {
			if !self_deleted_prefixes.iter().any(|p| p.as_slice() == prefix.as_slice()) {
				self_deleted_prefixes.push(prefix.clone());
			}
		}

		Ok(())
	}

	/// Create a child layer for nested modifications
	pub fn child(&self) -> LocalStorageLayer {
		self.clone()
	}
}
