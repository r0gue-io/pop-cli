// SPDX-License-Identifier: GPL-3.0

//! Error types for runtime executor operations.

use smoldot::executor::{host, runtime_call};
use thiserror::Error;

/// Errors that can occur during runtime execution.
#[derive(Debug, Error)]
pub enum ExecutorError {
	/// Error creating the VM prototype from WASM code.
	#[error("Failed to create VM prototype: {message}")]
	PrototypeCreation {
		/// The error message describing the failure.
		message: String,
	},

	/// Error starting the runtime call.
	#[error("Failed to start runtime call `{method}`: {message}")]
	StartError {
		/// The runtime method that failed to start.
		method: String,
		/// The error message describing the failure.
		message: String,
	},

	/// Error during runtime execution.
	#[error("Runtime execution error in `{method}`: {message}")]
	RuntimeError {
		/// The runtime method that failed.
		method: String,
		/// The error message describing the failure.
		message: String,
	},

	/// Storage operation failed.
	#[error("Storage operation failed for key {key}: {message}")]
	StorageError {
		/// The storage key that caused the error (hex-encoded).
		key: String,
		/// The error message describing the failure.
		message: String,
	},

	/// Invalid heap pages value in storage.
	#[error("Invalid heap pages value: {message}")]
	InvalidHeapPages {
		/// The error message describing the invalid value.
		message: String,
	},
}

impl From<host::NewErr> for ExecutorError {
	fn from(err: host::NewErr) -> Self {
		ExecutorError::PrototypeCreation { message: err.to_string() }
	}
}

impl From<runtime_call::Error> for ExecutorError {
	fn from(err: runtime_call::Error) -> Self {
		ExecutorError::RuntimeError { method: String::new(), message: err.to_string() }
	}
}
