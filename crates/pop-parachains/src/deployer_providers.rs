// SPDX-License-Identifier: GPL-3.0

use std::time::{SystemTime, UNIX_EPOCH};
use strum::{EnumMessage as _, EnumProperty as _};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

/// Supported deployment providers.
#[derive(
	AsRefStr,
	Clone,
	Debug,
	Display,
	EnumMessage,
	EnumString,
	EnumProperty,
	Eq,
	PartialEq,
	VariantArray,
)]
pub enum DeploymentProvider {
	/// Polkadot Deployment Portal (PDP). This provider enables seamless deployment of Polkadot
	/// Native Rollups through the Polkadot Cloud.
	#[strum(
		ascii_case_insensitive,
		serialize = "pdp",
		message = "Polkadot Deployment Portal",
		detailed_message = "Effortlessly deploy Polkadot Native Rollups in the Polkadot Cloud.",
		props(
			BaseURL = "https://staging.deploypolkadot.xyz",
			CollatorKeysURI = "/api/public/v1/parachains/{para_id}/collators/{chain_name}",
			DeployURI = "/api/public/v1/parachains/{para_id}/resources",
		)
	)]
	PDP,
}

impl DeploymentProvider {
	/// Returns the name of the provider.
	pub fn name(&self) -> &'static str {
		self.get_message().unwrap_or_default()
	}

	/// Returns the detailed description of the provider.
	pub fn description(&self) -> &'static str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Returns the base URL of the provider.
	pub fn base_url(&self) -> &'static str {
		self.get_str("BaseURL").unwrap_or("")
	}

	/// Constructs the full URI for querying collator keys.
	///
	/// # Arguments
	/// * `chain_name` - The specific chain name associated with the parachain.
	/// * `para_id` - The parachain ID for which collator keys are being requested.
	pub fn get_collator_keys_path(&self, chain_name: &str, para_id: u32) -> String {
		self.get_str("CollatorKeysURI")
			.map(|template| {
				template
					.replace("{para_id}", &para_id.to_string())
					.replace("{chain_name}", chain_name)
			})
			.unwrap_or_else(|| "".to_string())
	}

	/// Constructs the full URI for deploying a parachain.
	///
	/// # Arguments
	/// * `para_id` - The parachain ID that is being deployed.
	pub fn get_deploy_path(&self, para_id: u32) -> String {
		self.get_str("DeployURI")
			.map(|template| template.replace("{para_id}", &para_id.to_string()))
			.unwrap_or_else(|| "".to_string())
	}
}

/// Supported chains with its public RPC endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, Display, VariantArray)]
pub enum SupportedChains {
	PASEO,
	WESTEND,
	KUSAMA,
	POLKADOT,
}

impl SupportedChains {
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
	// List of RPC URLs for a given chain.
	fn rpc_urls(&self) -> Vec<&'static str> {
		match self {
			SupportedChains::PASEO => &[
				"wss://paseo.dotters.network",
				"wss://rpc.ibp.network/paseo",
				"wss://pas-rpc.stakeworld.io",
				"wss://paseo-rpc.dwellir.com",
				"wss://paseo.rpc.amforc.com",
			],
			SupportedChains::WESTEND => &[
				"wss://westend-rpc.polkadot.io",
				"wss://westend-rpc.dwellir.com",
				"wss://westend-rpc-tn.dwellir.com",
				"wss://rpc.ibp.network/westend",
				"wss://westend.dotters.network",
			],
			SupportedChains::KUSAMA => &[
				"wss://kusama-rpc.publicnode.com",
				"wss://kusama-rpc.dwellir.com",
				"wss://kusama-rpc-tn.dwellir.com",
				"wss://rpc.ibp.network/kusama",
				"wss://kusama.dotters.network",
			],
			SupportedChains::POLKADOT => &[
				"wss://polkadot-rpc.publicnode.com",
				"wss://polkadot-public-rpc.blockops.network/ws",
				"wss://polkadot-rpc.dwellir.com",
				"wss://rpc.ibp.network/polkadot",
				"wss://polkadot.dotters.network",
			],
		}
		.to_vec()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use strum::VariantArray;

	#[test]
	fn get_name_works() {
		assert_eq!(DeploymentProvider::PDP.name(), "Polkadot Deployment Portal");
	}

	#[test]
	fn get_description_works() {
		assert_eq!(
			DeploymentProvider::PDP.description(),
			"Effortlessly deploy Polkadot Native Rollups in the Polkadot Cloud."
		);
	}

	#[test]
	fn base_url_works() {
		let provider = DeploymentProvider::PDP;
		assert_eq!(provider.base_url(), "https://staging.deploypolkadot.xyz");
	}

	#[test]
	fn get_collator_keys_path_works() {
		let provider = DeploymentProvider::PDP;
		assert_eq!(
			provider.get_collator_keys_path("paseo", 2000),
			"/api/public/v1/parachains/2000/collators/paseo"
		);
	}

	#[test]
	fn get_deploy_path_works() {
		let provider = DeploymentProvider::PDP;
		assert_eq!(provider.get_deploy_path(2000), "/api/public/v1/parachains/2000/resources");
	}

	#[test]
	fn ensures_get_rpc_url_returns_valid_url() {
		for chain in SupportedChains::VARIANTS {
			let rpc = chain.get_rpc_url();
			assert!(rpc.is_some() && chain.rpc_urls().contains(&rpc.as_deref().unwrap()));
		}
	}
}
