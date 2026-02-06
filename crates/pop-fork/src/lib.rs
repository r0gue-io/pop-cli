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
//! ## Blockchain Manager
//!
//! - [`Blockchain`] - Main entry point for creating and managing forked chains
//! - [`ChainType`] - Identifies whether the chain is a relay chain or parachain
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
//!
//! ## Transaction Pool
//!
//! - [`TxPool`] - Minimal FIFO queue for pending extrinsics

mod block;
mod blockchain;
mod builder;
mod cache;
pub mod error;
pub mod executor;
pub mod inherent;
mod local;
mod models;
mod remote;
mod rpc;
pub mod rpc_server;
mod schema;
mod strings;
mod txpool;

#[cfg(test)]
pub mod testing;

pub use block::{Block, BlockForkPoint};
pub use blockchain::{
	Blockchain, BlockchainError, BlockchainEvent, BuildBlockResult, ChainType, FailedExtrinsic,
	InvalidTransaction, TransactionValidity, TransactionValidityError, UnknownTransaction,
	ValidTransaction,
};
pub use builder::{
	ApplyExtrinsicResult, BlockBuilder, ConsensusEngineId, DigestItem, consensus_engine,
	create_next_header, create_next_header_with_slot,
};
pub use cache::{PrefixScanProgress, StorageCache};
pub use error::{
	BlockBuilderError, BlockError, CacheError, ExecutorError, LocalStorageError,
	RemoteStorageError, RpcClientError, TxPoolError,
};
pub use executor::{
	ExecutorConfig, RuntimeCallResult, RuntimeExecutor, RuntimeLog, RuntimeVersion,
	SignatureMockMode,
};
pub use inherent::{InherentProvider, ParachainInherent, TimestampInherent, default_providers};
pub use local::LocalStorageLayer;
pub use models::BlockRow;
pub use remote::RemoteStorageLayer;
pub use rpc::ForkRpcClient;
pub use txpool::TxPool;
