// SPDX-License-Identifier: GPL-3.0

//! Error types for fork operations.
//!
//! This module contains all error types used throughout the `pop-fork` crate,
//! organized by context:
//!
//! - [`block::BlockError`] - Errors from block operations.
//! - [`builder::BlockBuilderError`] - Errors from block builder operations.
//! - [`cache::CacheError`] - Errors from SQLite storage cache operations.
//! - [`executor::ExecutorError`] - Errors from runtime executor operations.
//! - [`local::LocalStorageError`] - Errors from local storage layer operations.
//! - [`remote::RemoteStorageError`] - Errors from remote storage layer operations.
//! - [`rpc::RpcClientError`] - Errors from RPC client operations.

pub mod block;
pub mod builder;
pub mod cache;
pub mod executor;
pub mod local;
pub mod remote;
pub mod rpc;
pub mod txpool;

pub use block::BlockError;
pub use builder::BlockBuilderError;
pub use cache::CacheError;
pub use executor::ExecutorError;
pub use local::LocalStorageError;
pub use remote::RemoteStorageError;
pub use rpc::RpcClientError;
pub use txpool::TxPoolError;
