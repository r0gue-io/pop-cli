// SPDX-License-Identifier: GPL-3.0

//! Fork functionality for creating local forks of live Polkadot SDK chains.
//!
//! This crate provides the infrastructure for lazy-loading state from live chains,
//! enabling instant local forks without full state sync.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         pop fork chain                          │
//! │                              CLI                                 │
//! └─────────────────────────────────────────────────────────────────┘
//!                                 │
//!                                 ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                         RPC Server                               │
//! │           (Polkadot SDK compatible JSON-RPC)                     │
//! └─────────────────────────────────────────────────────────────────┘
//!                                 │
//!                                 ▼
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                     Layered Storage                              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
//! │  │ Local Layer │─▶│ Cache Layer │─▶│ Remote Layer (Live RPC) │  │
//! │  │(modifications)│ │  (SQLite)   │  │    (lazy fetch)         │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Main Types
//!
//! - [`ForkRpcClient`] - RPC client for connecting to live chains
//! - [`StorageCache`] - SQLite-based persistent cache for storage values
//! - [`RemoteStorageLayer`] - Cache-through layer that lazily fetches from RPC
//! - [`LocalStorageLayer`] - Local storage layer for tracking modifications to forked state
//! - [`RuntimeExecutor`] - Runtime executor for executing Substrate runtime calls

mod cache;
pub mod error;
pub mod executor;
mod local;
mod models;
mod remote;
mod rpc;
mod schema;
mod strings;

pub use cache::{PrefixScanProgress, StorageCache};
pub use error::{CacheError, ExecutorError, LocalStorageError, RemoteStorageError, RpcClientError};
pub use executor::{
	ExecutorConfig, RuntimeCallResult, RuntimeExecutor, RuntimeLog, RuntimeVersion,
	SignatureMockMode,
};
pub use local::LocalStorageLayer;
pub use models::BlockRow;
pub use remote::RemoteStorageLayer;
pub use rpc::ForkRpcClient;
