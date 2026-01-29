// SPDX-License-Identifier: GPL-3.0

/// Contains benchmarking utilities.
#[cfg(feature = "chain")]
pub mod bench;
/// Contains utilities for sourcing binaries.
pub mod binary;
pub mod builds;
#[cfg(feature = "chain")]
pub mod chain;
#[cfg(feature = "contract")]
pub mod contracts;
#[cfg(any(feature = "chain", feature = "contract"))]
pub mod helpers;
/// Contains omni-node utilities.
#[cfg(feature = "chain")]
pub mod omni_node;
/// Contains utilities for interacting with the CLI prompt.
pub mod prompt;
/// Contains utilities for interacting with RPC nodes.
pub mod rpc;
/// Contains runtime utilities.
#[cfg(feature = "chain")]
pub mod runtime;
/// Contains try-runtime utilities.
#[cfg(feature = "chain")]
pub mod try_runtime;
#[cfg(feature = "wallet-integration")]
pub mod wallet;

pub mod urls {
	/// Local dev node (Substrate default port 9944).
	pub const LOCAL: &str = "ws://localhost:9944/";
	/// Polkadot mainnet public RPC.
	#[cfg(all(feature = "chain", test))]
	pub const POLKADOT: &str = "wss://polkadot-rpc.publicnode.com/";
	/// Paseo testnet public RPC.
	#[cfg(feature = "chain")]
	pub const PASEO: &str = "wss://paseo.rpc.amforc.com/";
}
