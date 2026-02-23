// SPDX-License-Identifier: GPL-3.0

use std::time::{SystemTime, UNIX_EPOCH};
use strum_macros::{Display, EnumString, VariantArray};

/// Supported chains with its public RPC endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display, VariantArray, clap::ValueEnum)]
#[allow(non_camel_case_types)]
pub enum SupportedChains {
	/// Paseo.
	PASEO,
	/// Westend.
	WESTEND,
	/// Kusama.
	KUSAMA,
	/// Polkadot.
	POLKADOT,
	/// Asset Hub (Polkadot).
	#[value(name = "asset-hub-polkadot", alias = "asset-hub")]
	ASSET_HUB_POLKADOT,
	/// Asset Hub (Kusama).
	#[value(name = "asset-hub-kusama")]
	ASSET_HUB_KUSAMA,
	/// Asset Hub (Paseo).
	#[value(name = "asset-hub-paseo")]
	ASSET_HUB_PASEO,
	/// Asset Hub (Westend).
	#[value(name = "asset-hub-westend")]
	ASSET_HUB_WESTEND,
}

// Define static constants for RPC URLs
const PASEO_RPC_URLS: &[&str] = &[
	"wss://paseo.dotters.network",
	"wss://rpc.ibp.network/paseo",
	"wss://pas-rpc.stakeworld.io",
	"wss://paseo-rpc.dwellir.com",
	"wss://paseo.rpc.amforc.com",
];

const WESTEND_RPC_URLS: &[&str] = &[
	"wss://westend-rpc.polkadot.io",
	"wss://westend-rpc.dwellir.com",
	"wss://westend-rpc-tn.dwellir.com",
	"wss://rpc.ibp.network/westend",
	"wss://westend.dotters.network",
];

const KUSAMA_RPC_URLS: &[&str] = &[
	"wss://kusama-rpc.publicnode.com",
	"wss://kusama-rpc.dwellir.com",
	"wss://kusama-rpc-tn.dwellir.com",
	"wss://rpc.ibp.network/kusama",
	"wss://kusama.dotters.network",
];

const POLKADOT_RPC_URLS: &[&str] = &[
	"wss://polkadot-rpc.publicnode.com",
	"wss://polkadot-public-rpc.blockops.network/ws",
	"wss://polkadot-rpc.dwellir.com",
	"wss://rpc.ibp.network/polkadot",
	"wss://polkadot.dotters.network",
];

const ASSET_HUB_POLKADOT_RPC_URLS: &[&str] = &[
	"wss://polkadot-asset-hub-rpc.polkadot.io",
	"wss://asset-hub-polkadot-rpc.n.dwellir.com",
	"wss://sys.ibp.network/asset-hub-polkadot",
	"wss://rpc-asset-hub-polkadot.luckyfriday.io",
	"wss://asset-hub-polkadot.dotters.network",
];

const ASSET_HUB_KUSAMA_RPC_URLS: &[&str] = &[
	"wss://kusama-asset-hub-rpc.polkadot.io",
	"wss://asset-hub-kusama-rpc.n.dwellir.com",
	"wss://sys.ibp.network/asset-hub-kusama",
	"wss://rpc-asset-hub-kusama.luckyfriday.io",
	"wss://asset-hub-kusama.dotters.network",
];

const ASSET_HUB_PASEO_RPC_URLS: &[&str] = &[
	"wss://asset-hub-paseo.dotters.network",
	"wss://sys.ibp.network/asset-hub-paseo",
	"wss://asset-hub-paseo-rpc.n.dwellir.com",
	"wss://sys.turboflakes.io/asset-hub-paseo",
];

const ASSET_HUB_WESTEND_RPC_URLS: &[&str] =
	&["wss://westend-asset-hub-rpc.polkadot.io", "wss://asset-hub-westend-rpc.n.dwellir.com"];

impl SupportedChains {
	/// Returns whether this chain is a relay chain.
	pub fn is_relay(&self) -> bool {
		matches!(
			self,
			SupportedChains::PASEO |
				SupportedChains::WESTEND |
				SupportedChains::KUSAMA |
				SupportedChains::POLKADOT
		)
	}

	/// Selects a RPC URL for the chain.
	pub fn get_rpc_url(&self) -> Option<String> {
		let chain_urls = self.rpc_urls();
		// Select a pseudo-random index to provide a simple way to rotate URLs.
		chain_urls
			.get(
				(SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_millis() as usize) %
					chain_urls.len(),
			)
			.map(|s| s.to_string())
	}
	/// Returns a static list of RPC URLs for the chain.
	pub fn rpc_urls(&self) -> &'static [&'static str] {
		match self {
			SupportedChains::PASEO => PASEO_RPC_URLS,
			SupportedChains::WESTEND => WESTEND_RPC_URLS,
			SupportedChains::KUSAMA => KUSAMA_RPC_URLS,
			SupportedChains::POLKADOT => POLKADOT_RPC_URLS,
			SupportedChains::ASSET_HUB_POLKADOT => ASSET_HUB_POLKADOT_RPC_URLS,
			SupportedChains::ASSET_HUB_KUSAMA => ASSET_HUB_KUSAMA_RPC_URLS,
			SupportedChains::ASSET_HUB_PASEO => ASSET_HUB_PASEO_RPC_URLS,
			SupportedChains::ASSET_HUB_WESTEND => ASSET_HUB_WESTEND_RPC_URLS,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use strum::VariantArray;

	#[test]
	fn ensures_get_rpc_url_returns_valid_url() {
		for chain in SupportedChains::VARIANTS {
			let rpc = chain.get_rpc_url();
			assert!(rpc.is_some() && chain.rpc_urls().contains(&rpc.as_deref().unwrap()));
		}
	}

	#[test]
	fn rpc_urls_returns_valid_wss_endpoints_for_all_variants() {
		for chain in SupportedChains::VARIANTS {
			let urls = chain.rpc_urls();
			assert!(!urls.is_empty(), "rpc_urls() should return at least one URL for {:?}", chain);
			for url in urls {
				assert!(
					url.starts_with("wss://"),
					"RPC URL should use wss:// scheme, got: {}",
					url
				);
			}
		}
	}
}
