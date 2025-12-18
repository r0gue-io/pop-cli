// SPDX-License-Identifier: GPL-3.0

//! Error types for fork operations.
//!
//! This module contains all error types used throughout the `pop-fork` crate,
//! organized by context:
//!
//! - [`cache::CacheError`] - Errors from SQLite storage cache operations.
//! - [`rpc::RpcClientError`] - Errors from RPC client operations.
//! - [`remote::RemoteStorageError`] - Errors from remote storage layer operations.

pub mod cache;
pub mod remote;
pub mod rpc;
pub mod local;

pub use cache::CacheError;
pub use remote::RemoteStorageError;
pub use local::LocalStorageError;
pub use rpc::RpcClientError;
