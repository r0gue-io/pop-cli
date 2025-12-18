// SPDX-License-Identifier: GPL-3.0

use crate::{cache::StorageCache, remote::RemoteStorageLayer, rpc::ForkRpcClient};
use std::{
	collections::HashMap,
	sync::{Arc, RwLock},
};

#[derive(Clone, Debug)]
pub struct LocalStorageLayer {
	parent: RemoteStorageLayer,
	modifications: Arc<RwLock<HashMap<Vec<u8>, Option<Vec<u8>>>>>,
	deleted_prefixes: Arc<RwLock<Vec<Vec<u8>>>>,
}

impl LocalStorageLayer {
	fn new(parent: RemoteStorageLayer) -> Self {
		Self {
			parent,
			modifications: Arc::new(RwLock::new(HashMap::new())),
			deleted_prefixes: Arc::new(RwLock::new(Vec::new())),
		}
	}
}
