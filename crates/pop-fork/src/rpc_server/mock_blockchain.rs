// SPDX-License-Identifier: GPL-3.0

//! Mock blockchain implementation for RPC server development.
//!
//! This module provides a minimal stub implementation that mirrors the real
//! `Blockchain` API from issue #829. All methods return empty/default values.
//!
//! When the real `Blockchain` implementation lands, this can be replaced by
//! simply changing the import.

use crate::{Block, BlockForkPoint, ExecutorConfig};
use std::sync::Arc;
use subxt::config::substrate::H256;
use url::Url;

/// Chain type classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainType {
	/// A relay chain (Polkadot, Kusama, etc.).
	RelayChain,
	/// A parachain with its para_id.
	Parachain {
		/// The parachain ID.
		para_id: u32,
	},
}

/// Error type for blockchain operations.
pub struct BlockchainError;

/// Temporary BC Manager stub until #829 lands.
///
/// This struct mirrors the real `Blockchain` API but returns empty/default values.
/// All methods that require actual blockchain state return `todo!()` or empty values.
pub struct MockBlockchain;

impl MockBlockchain {
	// --- Initialization ---

	/// Create a new blockchain forked from a live chain.
	///
	/// Stub: Returns an empty MockBlockchain.
	#[allow(unused_variables)]
	pub async fn fork(
		endpoint: &Url,
		cache_path: Option<&str>,
	) -> Result<Arc<Self>, BlockchainError> {
		Ok(Arc::new(Self))
	}

	/// Create a new blockchain forked from a live chain at a specific block.
	///
	/// Stub: Returns an empty MockBlockchain.
	#[allow(unused_variables)]
	pub async fn fork_at(
		endpoint: &Url,
		cache_path: Option<&str>,
		fork_point: Option<BlockForkPoint>,
	) -> Result<Arc<Self>, BlockchainError> {
		Ok(Arc::new(Self))
	}

	/// Create a new blockchain forked from a live chain with custom executor configuration.
	///
	/// Stub: Returns an empty MockBlockchain.
	#[allow(unused_variables)]
	pub async fn fork_with_config(
		endpoint: &Url,
		cache_path: Option<&str>,
		fork_point: Option<BlockForkPoint>,
		config: ExecutorConfig,
	) -> Result<Arc<Self>, BlockchainError> {
		Ok(Arc::new(Self))
	}

	// --- Chain Info ---

	/// Get the chain name.
	///
	/// Stub: Returns empty string.
	pub fn chain_name(&self) -> &str {
		""
	}

	/// Get the chain type.
	///
	/// Stub: Returns RelayChain.
	pub fn chain_type(&self) -> &ChainType {
		&ChainType::RelayChain
	}

	/// Get the fork point block hash.
	///
	/// Stub: Returns zero hash.
	pub fn fork_point(&self) -> H256 {
		H256::zero()
	}

	/// Get the fork point block number.
	///
	/// Stub: Returns 0.
	pub fn fork_point_number(&self) -> u32 {
		0
	}

	// --- Head Block ---

	/// Get the current head block.
	///
	/// Stub: Returns error (no block available).
	pub async fn head(&self) -> Result<Block, BlockchainError> {
		Err(BlockchainError)
	}

	/// Get the current head block number.
	///
	/// Stub: Returns 0.
	pub async fn head_number(&self) -> u32 {
		0
	}

	/// Get the current head block hash.
	///
	/// Stub: Returns zero hash.
	pub async fn head_hash(&self) -> H256 {
		H256::zero()
	}

	// --- Block Building ---

	/// Build a new block with the given extrinsics.
	///
	/// Stub: Not implemented.
	#[allow(unused_variables)]
	pub async fn build_block(&self, extrinsics: Vec<Vec<u8>>) -> Result<Block, BlockchainError> {
		Err(BlockchainError)
	}

	/// Build an empty block (just inherents, no user extrinsics).
	///
	/// Stub: Not implemented.
	pub async fn build_empty_block(&self) -> Result<Block, BlockchainError> {
		Err(BlockchainError)
	}

	// --- Runtime Calls ---

	/// Execute a runtime call at the current head.
	///
	/// Stub: Returns empty bytes.
	#[allow(unused_variables)]
	pub async fn call(&self, method: &str, args: &[u8]) -> Result<Vec<u8>, BlockchainError> {
		Ok(vec![])
	}

	// --- Storage ---

	/// Get storage value at the current head.
	///
	/// Stub: Returns None.
	#[allow(unused_variables)]
	pub async fn storage(&self, key: &[u8]) -> Result<Option<Vec<u8>>, BlockchainError> {
		Ok(None)
	}
}
