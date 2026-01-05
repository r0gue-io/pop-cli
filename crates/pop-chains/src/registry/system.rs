// SPDX-License-Identifier: GPL-3.0

use super::{traits::Requires, *};
use crate::{
	Error,
	traits::{Args, Binary},
};
use pop_common::{
	polkadot_sdk::sort_by_latest_stable_version,
	sourcing::{ArchiveFileSpec, GitHub::ReleaseArchive, Source, traits::Source as SourceT},
	target,
};

/// A chain containing core Polkadot protocol features.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/overview/> for more details.
#[derive(Clone)]
pub(crate) struct System;

impl SourceT for System {
	type Error = Error;
	fn source(&self) -> Result<Source, Error> {
		// Source from GitHub release asset
		let binary = self.binary();
		Ok(Source::GitHub(ReleaseArchive {
			owner: "r0gue-io".into(),
			repository: "polkadot".into(),
			tag: None,
			tag_pattern: Some("polkadot-{version}".into()),
			prerelease: false,
			version_comparator: sort_by_latest_stable_version,
			fallback: "stable2412".into(),
			archive: format!("{binary}-{}.tar.gz", target()?),
			contents: vec![ArchiveFileSpec::new(binary.into(), None, true)],
			latest: None,
		}))
	}
}

impl Binary for System {
	fn binary(&self) -> &'static str {
		"polkadot-parachain"
	}
}

impl Args for System {
	fn args(&self) -> Option<Vec<&str>> {
		Some(vec!["-lxcm=trace"])
	}
}

// Macro for reducing boilerplate code.
macro_rules! impl_system_chain {
	($name:ident) => {
		impl_chain!($name);
		impl Requires for $name {}

		impl SourceT for $name {
			type Error = Error;
			fn source(&self) -> Result<Source, Error> {
				SourceT::source(&System)
			}
		}

		impl Binary for $name {
			fn binary(&self) -> &'static str {
				"polkadot-parachain"
			}
		}

		impl Args for $name {
			fn args(&self) -> Option<Vec<&str>> {
				System.args()
			}
		}

		impl GenesisOverrides for $name {}
	};
}

/// The Asset Hub enables the management of fungible and non-fungible assets across the network.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/asset-hub/> for more details.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetHub(pub(super) Chain);
impl AssetHub {
	/// A new instance of the Asset Hub.
	///
	/// # Arguments
	/// * `id` - The chain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Chain::new("asset-hub", id, format!("asset-hub-{}", relay.chain())))
	}
}
impl_system_chain!(AssetHub);

/// The Bridge Hub facilitates trustless interactions between Polkadot, Kusama, Ethereum, and
/// other blockchain ecosystems.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/bridge-hub/> for more details.
#[derive(Clone)]
pub struct BridgeHub(Chain);
impl BridgeHub {
	/// A new instance of the Bridge Hub.
	///
	/// # Arguments
	/// * `id` - The chain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Chain::new("bridge-hub", id, format!("bridge-hub-{}", relay.chain())))
	}
}
impl_system_chain!(BridgeHub);

/// The Collectives chain operates as a dedicated chain exclusive to the Polkadot network.
/// This specialized infrastructure provides a foundation for various on-chain governance groups
/// essential to Polkadot's ecosystem.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/collectives/> for more details.
#[derive(Clone)]
pub struct Collectives(Chain);
impl Collectives {
	/// A new instance of the Collectives chain.
	///
	/// # Arguments
	/// * `id` - The chain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Chain::new("collectives", id, format!("collectives-{}", relay.chain())))
	}
}
impl_system_chain!(Collectives);

/// The Coretime system chain facilitates the allocation, procurement, sale, and scheduling of
/// bulk coretime, enabling tasks (such as chains) to utilize the computation and security
/// provided by Polkadot.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/coretime/> for more details.
#[derive(Clone)]
pub struct Coretime(Chain);
impl Coretime {
	/// A new instance of the Coretime chain.
	///
	/// # Arguments
	/// * `id` - The chain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Chain::new("coretime", id, format!("coretime-{}", relay.chain())))
	}
}
impl_system_chain!(Coretime);

/// The People system chain is a specialized chain within the Polkadot ecosystem dedicated
/// to secure, decentralized identity management.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/people/> for more details.
#[derive(Clone)]
pub struct People(Chain);
impl People {
	/// A new instance of the People chain.
	///
	/// # Arguments
	/// * `id` - The chain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Chain::new("people", id, format!("people-{}", relay.chain())))
	}
}
impl_system_chain!(People);

/// The PassetHub system chain is a temporary chain within the Polkadot ecosystem dedicated
/// to deploy smart contracts..
///
/// See <https://docs.polkadot.com/polkadot-protocol/smart-contract-basics/networks/#passet-hub> for more details.
#[derive(Clone)]
pub struct PassetHub(Chain);
impl PassetHub {
	/// A new instance of the PassetHub chain.
	///
	/// # Arguments
	/// * `id` - The chain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Chain::new("passet-hub", id, format!("passet-hub-{}", relay.chain())))
	}
}
impl_system_chain!(PassetHub);

#[cfg(test)]
mod tests {
	use super::*;
	use pop_common::SortedSlice;
	use std::ptr::fn_addr_eq;

	#[test]
	fn source_works() {
		let system = System;
		assert!(matches!(
			system.source().unwrap(),
			Source::GitHub(ReleaseArchive { owner, repository, tag, tag_pattern, prerelease, version_comparator, fallback, archive, contents, latest })
				if owner == "r0gue-io" &&
					repository == "polkadot" &&
					tag.is_none() &&
					tag_pattern == Some("polkadot-{version}".into()) &&
					!prerelease &&
					fn_addr_eq(version_comparator, sort_by_latest_stable_version as for<'a> fn(&'a mut [String]) -> SortedSlice<'a, String>) &&
					fallback == "stable2412" &&
					archive == format!("polkadot-parachain-{}.tar.gz", target().unwrap()) &&
					contents == vec![ArchiveFileSpec::new("polkadot-parachain".into(), None, true)] &&
					latest.is_none()
		));
	}

	#[test]
	fn binary_works() {
		assert_eq!(System.binary(), "polkadot-parachain");
	}

	#[test]
	fn args_works() {
		assert_eq!(System.args().unwrap(), vec!["-lxcm=trace",]);
	}

	#[test]
	fn asset_hub_works() {
		let asset_hub = AssetHub::new(1_000, Relay::Paseo);
		assert_eq!(asset_hub.args(), System.args());
		assert_eq!(asset_hub.binary(), "polkadot-parachain");
		assert_eq!(asset_hub.chain(), "asset-hub-paseo-local");
		assert!(asset_hub.genesis_overrides().is_none());
		assert_eq!(asset_hub.name(), "asset-hub");
		assert_eq!(asset_hub.source().unwrap(), System.source().unwrap());
	}

	#[test]
	fn bridge_hub_works() {
		let bridge_hub = BridgeHub::new(1_002, Relay::Paseo);
		assert_eq!(bridge_hub.args(), System.args());
		assert_eq!(bridge_hub.binary(), "polkadot-parachain");
		assert_eq!(bridge_hub.chain(), "bridge-hub-paseo-local");
		assert!(bridge_hub.genesis_overrides().is_none());
		assert_eq!(bridge_hub.name(), "bridge-hub");
		assert_eq!(bridge_hub.source().unwrap(), System.source().unwrap());
	}

	#[test]
	fn collectives_works() {
		let collectives = Collectives::new(1_001, Relay::Paseo);
		assert_eq!(collectives.args(), System.args());
		assert_eq!(collectives.binary(), "polkadot-parachain");
		assert_eq!(collectives.chain(), "collectives-paseo-local");
		assert!(collectives.genesis_overrides().is_none());
		assert_eq!(collectives.name(), "collectives");
		assert_eq!(collectives.source().unwrap(), System.source().unwrap());
	}

	#[test]
	fn coretime_works() {
		let coretime = Coretime::new(1_001, Relay::Paseo);
		assert_eq!(coretime.args(), System.args());
		assert_eq!(coretime.binary(), "polkadot-parachain");
		assert_eq!(coretime.chain(), "coretime-paseo-local");
		assert!(coretime.genesis_overrides().is_none());
		assert_eq!(coretime.name(), "coretime");
		assert_eq!(coretime.source().unwrap(), System.source().unwrap());
	}

	#[test]
	fn people_works() {
		let people = People::new(1_001, Relay::Paseo);
		assert_eq!(people.args(), System.args());
		assert_eq!(people.binary(), "polkadot-parachain");
		assert_eq!(people.chain(), "people-paseo-local");
		assert!(people.genesis_overrides().is_none());
		assert_eq!(people.name(), "people");
		assert_eq!(people.source().unwrap(), System.source().unwrap());
	}

	#[test]
	fn passet_hub_works() {
		let passethub = PassetHub::new(1_111, Relay::Paseo);
		assert_eq!(passethub.args(), System.args());
		assert_eq!(passethub.binary(), "polkadot-parachain");
		assert_eq!(passethub.chain(), "passet-hub-paseo-local");
		assert!(passethub.genesis_overrides().is_none());
		assert_eq!(passethub.name(), "passet-hub");
		assert_eq!(passethub.source().unwrap(), System.source().unwrap());
	}
}
