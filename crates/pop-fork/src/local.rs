// SPDX-License-Identifier: GPL-3.0

use crate::{error::LocalStorageError, remote::RemoteStorageLayer};
use std::{
	collections::HashMap,
	sync::{Arc, RwLock},
};

type SharedStorageValue = Arc<Vec<u8>>;
type Modifications = HashMap<Vec<u8>, Option<SharedStorageValue>>;
type DeletedPrefixes = Vec<Vec<u8>>;

#[derive(Clone, Debug)]
pub struct LocalStorageLayer {
	parent: RemoteStorageLayer,
	modifications: Arc<RwLock<Modifications>>,
	deleted_prefixes: Arc<RwLock<DeletedPrefixes>>,
}

impl LocalStorageLayer {
	fn new(parent: RemoteStorageLayer) -> Self {
		Self {
			parent,
			modifications: Arc::new(RwLock::new(HashMap::new())),
			deleted_prefixes: Arc::new(RwLock::new(Vec::new())),
		}
	}

    fn get(&self, key: &[u8]) -> Result<Option<SharedStorageValue>, LocalStorageError>{
        let modifications_lock = self.modifications.try_read().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
        match modifications_lock.get(key){
            Some(value)  => Ok(value.clone()),
            _ => Ok(None)
        }
    }
}
