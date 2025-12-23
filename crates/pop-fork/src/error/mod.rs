// SPDX-License-Identifier: GPL-3.0

//! Error types for fork operations.
//!
//! This module contains all error types used throughout the `pop-fork` crate,
//! organized by context:
//!
//! - [`cache::CacheError`] - Errors from SQLite storage cache operations.
//! - [`executor::ExecutorError`] - Errors from runtime executor operations.
//! - [`rpc::RpcClientError`] - Errors from RPC client operations.
//! - [`remote::RemoteStorageError`] - Errors from remote storage layer operations.

pub mod cache;
pub mod executor;
pub mod local;
pub mod remote;
pub mod rpc;

pub use cache::CacheError;
pub use executor::ExecutorError;
pub use local::LocalStorageError;
pub use remote::RemoteStorageError;
pub use rpc::RpcClientError;
