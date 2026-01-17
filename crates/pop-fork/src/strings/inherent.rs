// SPDX-License-Identifier: GPL-3.0

//! String constants for the inherent module.

/// String constants for the timestamp inherent provider.
pub mod timestamp {
	/// Provider identifier for logging/debugging.
	pub const IDENTIFIER: &str = "Timestamp";

	/// Storage key components for reading the current timestamp.
	pub mod storage_keys {
		/// Pallet name for computing the storage key prefix.
		pub const PALLET_NAME: &[u8] = b"Timestamp";

		/// Storage item name for the current timestamp.
		pub const NOW: &[u8] = b"Now";
	}

	/// Error message formats.
	pub mod errors {
		/// Format string for timestamp decode failures.
		pub const DECODE_FAILED: &str = "Failed to decode timestamp";
	}
}

/// String constants for the parachain inherent provider.
pub mod parachain {
	/// Provider identifier for logging/debugging.
	pub const IDENTIFIER: &str = "ParachainSystem";
}
