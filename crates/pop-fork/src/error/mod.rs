// SPDX-License-Identifier: GPL-3.0

//! Error types for fork operations.
//!
//! This module contains all error types used throughout the `pop-fork` crate,
//! organized by context:
//!
//! - [`cache::CacheError`] - Errors from SQLite storage cache operations.
//! - [`rpc::RpcClientError`] - Errors from RPC client operations.

pub mod cache;
pub mod rpc;

pub use cache::CacheError;
pub use rpc::RpcClientError;
