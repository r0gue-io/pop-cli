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
//! ## Block and Block Building
//!
//! - [`Block`] - Represents a block in the forked chain with its storage state
//! - [`BlockBuilder`] - Constructs new blocks by applying inherents and extrinsics
//! - [`InherentProvider`] - Trait for generating inherent (timestamp, etc.)
//!
//! ## Storage Layers
//!
//! - [`LocalStorageLayer`] - Tracks local modifications to forked state
//! - [`RemoteStorageLayer`] - Cache-through layer that lazily fetches from RPC
//! - [`StorageCache`] - SQLite-based persistent cache for storage values
//!
//! ## Runtime Execution
//!
//! - [`RuntimeExecutor`] - Executes Polkadot SDK runtime calls against forked state
//! - [`ForkRpcClient`] - RPC client for connecting to live chains

mod block;
mod builder;
mod cache;
pub mod error;
pub mod executor;
pub mod inherent;
mod local;
mod models;
mod remote;
mod rpc;
mod schema;
mod strings;

pub use block::{Block, BlockForkPoint};
pub use builder::{
	ApplyExtrinsicResult, BlockBuilder, ConsensusEngineId, DigestItem, consensus_engine,
	create_next_header,
};
pub use cache::{PrefixScanProgress, StorageCache};
pub use error::{
	BlockBuilderError, BlockError, CacheError, ExecutorError, LocalStorageError,
	RemoteStorageError, RpcClientError,
};
pub use executor::{
	ExecutorConfig, RuntimeCallResult, RuntimeExecutor, RuntimeLog, RuntimeVersion,
	SignatureMockMode,
};
pub use inherent::InherentProvider;
pub use local::LocalStorageLayer;
pub use models::BlockRow;
pub use remote::RemoteStorageLayer;
pub use rpc::ForkRpcClient;
