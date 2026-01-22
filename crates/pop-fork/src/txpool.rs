// SPDX-License-Identifier: GPL-3.0

//! Minimal transaction pool for collecting submitted extrinsics.
//!
//! This is a simple FIFO queue with no validation or ordering,
//! designed for fork/simulation tools where production-grade
//! transaction pool complexity is unnecessary.

use crate::TxPoolError;
use std::sync::RwLock;
use subxt::config::substrate::H256;

/// A minimal transaction pool that stores pending extrinsics.
///
/// Thread-safe FIFO queue for extrinsics awaiting inclusion in a block.
/// Does not perform validation or ordering - extrinsics are processed
/// in submission order.
#[derive(Default)]
pub struct TxPool {
	pending: RwLock<Vec<Vec<u8>>>,
}

impl TxPool {
	/// Create a new empty transaction pool.
	pub fn new() -> Self {
		Self { pending: RwLock::new(Vec::new()) }
	}

	/// Submit an extrinsic to the pool.
	///
	/// Returns the blake2-256 hash of the extrinsic.
	pub fn submit(&self, extrinsic: Vec<u8>) -> Result<H256, TxPoolError> {
		let hash = H256::from(sp_core::blake2_256(&extrinsic));
		self.pending
			.write()
			.map_err(|err| TxPoolError::Lock(err.to_string()))?
			.push(extrinsic);
		Ok(hash)
	}

	/// Drain all pending extrinsics from the pool.
	///
	/// Returns all extrinsics in FIFO order and clears the pool.
	/// Used by block builder to collect transactions for inclusion.
	pub fn drain(&self) -> Result<Vec<Vec<u8>>, TxPoolError> {
		Ok(std::mem::take(
			&mut *self.pending.write().map_err(|err| TxPoolError::Lock(err.to_string()))?,
		))
	}

	/// Get a clone of all pending extrinsics without removing them.
	pub fn pending(&self) -> Result<Vec<Vec<u8>>, TxPoolError> {
		Ok(self.pending.read().map_err(|err| TxPoolError::Lock(err.to_string()))?.clone())
	}

	/// Returns the number of pending extrinsics.
	pub fn len(&self) -> Result<usize, TxPoolError> {
		Ok(self.pending.read().map_err(|err| TxPoolError::Lock(err.to_string()))?.len())
	}

	/// Returns true if there are no pending extrinsics.
	pub fn is_empty(&self) -> Result<bool, TxPoolError> {
		Ok(self.len()? == 0)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn submit_returns_correct_hash() {
		let pool = TxPool::new();
		let extrinsic = vec![1, 2, 3, 4];
		let expected_hash = H256::from(sp_core::blake2_256(&extrinsic));

		let hash = pool.submit(extrinsic).unwrap();

		assert_eq!(hash, expected_hash);
	}

	#[test]
	fn drain_returns_all_extrinsics_in_fifo_order() {
		let pool = TxPool::new();
		pool.submit(vec![1]).unwrap();
		pool.submit(vec![2]).unwrap();
		pool.submit(vec![3]).unwrap();

		let drained = pool.drain().unwrap();

		assert_eq!(drained, vec![vec![1], vec![2], vec![3]]);
		assert!(pool.is_empty().unwrap());
	}

	#[test]
	fn pending_returns_extrinsics_without_removing() {
		let pool = TxPool::new();
		pool.submit(vec![1, 2]).unwrap();
		pool.submit(vec![3, 4]).unwrap();

		let pending = pool.pending().unwrap();

		assert_eq!(pending, vec![vec![1, 2], vec![3, 4]]);
		assert_eq!(pool.len().unwrap(), 2);
	}
}
