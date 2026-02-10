// SPDX-License-Identifier: GPL-3.0

//! String constants for the RPC server module.
//!
//! These constants are used across RPC method implementations.

/// System RPC method constants.
pub mod system {
	/// Node name returned by `system_name`.
	pub const NODE_NAME: &str = "pop-fork";

	/// Node version returned by `system_version`.
	pub const NODE_VERSION: &str = "1.0.0";

	/// Mock libp2p peer ID returned by `system_localPeerId`.
	/// This is a valid ed25519 peer ID format but not connected to any real network.
	pub const MOCK_PEER_ID: &str = "12D3KooWBmAwcd4PJNJvfV89HwE48nwkRmAgo8Vy3uQEyNNHBox2";

	/// Node role returned by `system_nodeRoles`.
	pub const NODE_ROLE_FULL: &str = "Full";

	/// Chain type returned by `system_chainType`.
	pub const CHAIN_TYPE_DEVELOPMENT: &str = "Development";
}

/// Storage-related constants.
pub mod storage {
	/// Pallet name for System storage queries.
	pub const SYSTEM_PALLET: &[u8] = b"System";

	/// Storage item name for Account storage.
	pub const ACCOUNT_STORAGE: &[u8] = b"Account";

	/// Storage item name for Number storage (block number).
	pub const NUMBER_STORAGE: &[u8] = b"Number";

	/// Pallet name for Sudo storage queries.
	pub const SUDO_PALLET: &[u8] = b"Sudo";

	/// Storage item name for Sudo key.
	pub const SUDO_KEY_STORAGE: &[u8] = b"Key";

	/// Size of nonce field in AccountInfo (u32 = 4 bytes).
	pub const NONCE_SIZE: usize = 4;

	/// Special storage key for runtime WASM code.
	/// Changes to this key indicate a runtime upgrade.
	pub const RUNTIME_CODE_KEY: &[u8] = b":code";
}

/// Runtime API method names.
pub mod runtime_api {
	/// Runtime API method for fetching runtime version.
	pub const CORE_VERSION: &str = "Core_version";

	/// Prefix shared by all `Metadata_*` runtime API methods.
	///
	/// Used to route metadata calls to the upstream proxy for performance.
	pub const METADATA_PREFIX: &str = "Metadata_";

	/// Runtime API method for fetching metadata.
	pub const METADATA: &str = "Metadata_metadata";

	/// Runtime API method for querying transaction fee info.
	pub const QUERY_INFO: &str = "TransactionPaymentApi_query_info";

	/// Runtime API method for querying detailed fee breakdown.
	pub const QUERY_FEE_DETAILS: &str = "TransactionPaymentApi_query_fee_details";
}

/// Transaction-related constants.
pub mod transaction {
	/// Prefix for transaction operation IDs.
	pub const OPERATION_ID_PREFIX: &str = "tx-op";
}

/// ChainHead subscription limits.
pub mod chain_head {
	/// Maximum number of concurrent chainHead follow subscriptions.
	///
	/// Matches polkadot-sdk's default of 1024 subscriptions per connection
	/// (defined in substrate/client/cli/src/config.rs as RPC_DEFAULT_MAX_SUBS_PER_CONN).
	pub const MAX_SUBSCRIPTIONS: usize = 1024;

	/// Maximum number of concurrent operations per subscription.
	pub const MAX_OPERATIONS: usize = 16;
}
