// SPDX-License-Identifier: GPL-3.0

//! String constants for the RPC client module.

/// JSON-RPC method names used for error reporting.
///
/// These match the actual RPC method names in the Polkadot SDK JSON-RPC specification.
pub mod methods {
	pub const CHAIN_GET_FINALIZED_HEAD: &str = "chain_getFinalisedHead";
	pub const CHAIN_GET_HEADER: &str = "chain_getHeader";
	pub const STATE_GET_STORAGE: &str = "state_getStorage";
	pub const STATE_QUERY_STORAGE_AT: &str = "state_queryStorageAt";
	pub const STATE_GET_KEYS_PAGED: &str = "state_getKeysPaged";
	pub const STATE_GET_METADATA: &str = "state_getMetadata";
	pub const SYSTEM_CHAIN: &str = "system_chain";
	pub const SYSTEM_PROPERTIES: &str = "system_properties";
}

/// Well-known storage key identifiers.
pub mod storage_keys {
	/// The `:code` storage key containing the runtime WASM blob.
	pub const CODE: &str = ":code";
}
