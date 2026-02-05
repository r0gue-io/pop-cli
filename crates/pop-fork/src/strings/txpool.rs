// SPDX-License-Identifier: GPL-3.0

//! String constants for transaction pool and validation.

/// Runtime API method names for transaction validation.
pub mod runtime_api {
	/// Runtime method to validate a transaction before inclusion.
	///
	/// Takes encoded args: `(source, extrinsic, block_hash)`
	/// Returns: `TransactionValidity` (Result<ValidTransaction, TransactionValidityError>)
	pub const TAGGED_TRANSACTION_QUEUE_VALIDATE: &str =
		"TaggedTransactionQueue_validate_transaction";
}

/// Transaction source identifiers per Substrate spec.
pub mod transaction_source {
	/// Transaction originated outside the node (user submitted via RPC).
	pub const EXTERNAL: u8 = 0x02;

	/// Transaction originated from within the node (e.g., block author).
	// TODO: Remove #[allow(dead_code)] when used in future tasks.
	#[allow(dead_code)]
	pub const IN_BLOCK: u8 = 0x00;

	/// Transaction originated from a local source (e.g., off-chain worker).
	// TODO: Remove #[allow(dead_code)] when used in future tasks.
	#[allow(dead_code)]
	pub const LOCAL: u8 = 0x01;
}

/// Error messages for transaction validation.
// TODO: Remove #[allow(dead_code)] when used in Task 5 (author.rs validation).
#[allow(dead_code)]
pub mod error_messages {
	/// Message for invalid transaction errors.
	pub const INVALID_TRANSACTION: &str = "Transaction is invalid";

	/// Message for unknown transaction errors.
	pub const UNKNOWN_TRANSACTION: &str = "Transaction validity cannot be determined";
}
