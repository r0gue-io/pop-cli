// SPDX-License-Identifier: GPL-3.0

//! Storage provider trait.

use crate::error::RemoteStorageError;
use async_trait::async_trait;

/// Trait for storage providers.
#[async_trait]
pub trait StorageProvider: Send + Sync {
	/// Get storage value.
	async fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, RemoteStorageError>;
}

pub mod local;
pub mod remote;
