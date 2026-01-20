// SPDX-License-Identifier: GPL-3.0

//! Error types for block builder operations.

use super::{BlockError, ExecutorError, LocalStorageError};
use thiserror::Error;

/// Errors that can occur during block building.
#[derive(Debug, Error)]
pub enum BlockBuilderError {
	/// Error from the runtime executor.
	#[error("Executor error: {0}")]
	Executor(#[from] ExecutorError),

	/// Error from the storage layer.
	#[error("Storage error: {0}")]
	Storage(#[from] LocalStorageError),

	/// Error from block operations.
	#[error("Block error: {0}")]
	Block(#[from] BlockError),

	/// Error decoding data.
	#[error("Codec error: {0}")]
	Codec(String),

	/// Block has not been initialized yet.
	#[error("Block not initialized - call initialize() first")]
	NotInitialized,

	/// Block has already been initialized.
	#[error("Block already initialized - initialize() can only be called once")]
	AlreadyInitialized,

	/// Inherents have not been applied yet.
	#[error("Inherents not applied - call apply_inherents() before apply_extrinsic()")]
	InherentsNotApplied,

	/// Inherents have already been applied.
	#[error("Inherents already applied - apply_inherents() can only be called once")]
	InherentsAlreadyApplied,

	/// Inherent provider failed.
	#[error("Inherent provider `{provider}` failed: {message}")]
	InherentProvider {
		/// The identifier of the provider that failed.
		provider: String,
		/// The error message.
		message: String,
	},
}
