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
	fn new(parent: RemoteStorageLayer) -> Self {
		Self {
			parent,
			modifications: Arc::new(RwLock::new(HashMap::new())),
			deleted_prefixes: Arc::new(RwLock::new(Vec::new())),
		}
	}

    async fn get(&self, key: &[u8]) -> Result<Option<SharedValue>, LocalStorageError>{
        let modifications_lock = self.modifications.try_read().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
        let deleted_prefixes_lock = self.deleted_prefixes.try_read().map_err(|e| LocalStorageError::Lock(e.to_string()))?;
        match modifications_lock.get(key){
            Some(value)  => Ok(value.clone()),
            None if deleted_prefixes_lock.iter().any(|deleted| deleted.as_slice() == key)=> Ok(None),
             _ => Ok(self.parent.get(key).await?.map(|value| Arc::new(value)))
        }
    }

    fn set(&self, key: &[u8], value: Option<&[u8]>) -> Result<(), LocalStorageError>{
        let mut modifications_lock = self.modifications.try_write().map_err(|e| LocalStorageError::Lock(e.to_string()))?;

        modifications_lock.insert(key.to_vec(), value.map(|value| Arc::new(value.to_vec())));

        Ok(())
    }
}
