// SPDX-License-Identifier: GPL-3.0

use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use crate::remote::RemoteStorageLayer;
use crate::rpc::ForkRpcClient;
use crate::cache::StorageCache;

#[derive(Clone)]
pub struct LocalStorageLayer{
    parent: RemoteStorageLayer,
    modifications: Arc<RwLock<HashMap<Vec<u8>, Option<Vec<u8>>>>>,
    deleted_prefixes: Arc<RwLock<Vec<Vec<u8>>>>
}