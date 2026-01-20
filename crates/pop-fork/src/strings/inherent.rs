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

	/// Metadata lookup constants for dynamic pallet/call discovery.
	pub mod metadata {
		/// Pallet name for metadata lookup.
		pub const PALLET_NAME: &str = "Timestamp";

		/// Call name for the `set` function.
		pub const SET_CALL_NAME: &str = "set";
	}

	/// Constants for slot duration detection from runtime.
	pub mod slot_duration {
		/// Runtime API method for Aura slot duration.
		pub const AURA_API_METHOD: &str = "AuraApi_slot_duration";
	}

	/// Error message formats.
	pub mod errors {
		/// Format string for timestamp decode failures.
		pub const DECODE_FAILED: &str = "Failed to decode timestamp";

		/// Error when pallet is not found in metadata.
		pub const PALLET_NOT_FOUND: &str = "Pallet not found in metadata";

		/// Error when call is not found in pallet metadata.
		pub const CALL_NOT_FOUND: &str = "Call not found in pallet metadata";
	}
}

/// String constants for the parachain inherent provider.
pub mod parachain {
	/// Provider identifier for logging/debugging.
	pub const IDENTIFIER: &str = "ParachainSystem";

	/// Metadata lookup constants for dynamic pallet/call discovery.
	pub mod metadata {
		/// Pallet name for metadata lookup.
		pub const PALLET_NAME: &str = "ParachainSystem";

		/// Call name for the `set_validation_data` function.
		/// Reserved for future use when full parachain inherent is implemented.
		#[allow(dead_code)]
		pub const SET_VALIDATION_DATA_CALL_NAME: &str = "set_validation_data";
	}
}
