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

		/// Babe pallet name for metadata constant lookup.
		pub const BABE_PALLET: &str = "Babe";

		/// Babe constant name for expected block time.
		pub const BABE_EXPECTED_BLOCK_TIME: &str = "ExpectedBlockTime";

		/// Fallback slot duration for relay chains (6 seconds).
		pub const RELAY_CHAIN_FALLBACK_MS: u64 = 6_000;

		/// Fallback slot duration for parachains (12 seconds).
		pub const PARACHAIN_FALLBACK_MS: u64 = 12_000;
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
		pub const SET_VALIDATION_DATA_CALL_NAME: &str = "set_validation_data";
	}

	/// Storage key components for parachain info.
	pub mod storage_keys {
		/// Pallet name for computing the storage key prefix.
		pub const PARACHAIN_INFO_PALLET: &[u8] = b"ParachainInfo";

		/// Storage item name for the parachain ID.
		pub const PARACHAIN_ID: &[u8] = b"ParachainId";
	}
}

/// String constants for relay chain inherent mocking.
pub mod relay {
	/// Pallet name for ParaInherent (relay chain parachains inherent).
	pub const PARA_INHERENT_PALLET: &str = "ParaInherent";

	/// Storage key components for relay chain storage.
	pub mod storage_keys {
		/// Pallet name for computing the storage key prefix.
		pub const PARA_INHERENT_PALLET: &[u8] = b"ParaInherent";

		/// Storage item name for the Included flag.
		pub const INCLUDED: &[u8] = b"Included";
	}
}
