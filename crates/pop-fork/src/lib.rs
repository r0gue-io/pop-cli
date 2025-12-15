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

mod cache;
pub mod error;
mod models;
mod rpc;
mod strings;
mod schema;

pub use cache::StorageCache;
pub use error::{CacheError, RpcClientError};
pub use models::BlockRow;
pub use rpc::ForkRpcClient;
