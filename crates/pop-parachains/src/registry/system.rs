// SPDX-License-Identifier: GPL-3.0

use super::*;
use crate::traits::{Args, Binary};
use pop_common::{
	polkadot_sdk::sort_by_latest_stable_version,
	sourcing::{traits::Source as SourceT, GitHub::ReleaseArchive, Source},
	target,
};

/// A parachain containing core Polkadot protocol features.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/overview/> for more details.
#[derive(Clone)]
pub(crate) struct System;

impl SourceT for System {
	fn source(&self) -> Result<Source, pop_common::Error> {
		// Source from GitHub release asset
		let binary = self.binary();
		Ok(Source::GitHub(ReleaseArchive {
			owner: "r0gue-io".into(),
			repository: "polkadot".into(),
			tag: Some("polkadot-{tag}".into()),
			tag_pattern: Some("polkadot-{version}".into()),
			prerelease: false,
			version_comparator: sort_by_latest_stable_version,
			fallback: "stable2412".into(),
			archive: format!("{binary}-{}.tar.gz", target()?),
			contents: vec![(binary, None, true)],
			latest: None,
		}))
	}
}

impl Binary for System {
	fn binary(&self) -> &'static str {
		"polkadot-parachain"
	}
}

// Macro for reducing boilerplate code.
macro_rules! impl_system_parachain {
	($name:ident) => {
		impl_parachain!($name);
		impl Requires for $name {}

		impl SourceT for $name {
			fn source(&self) -> Result<Source, pop_common::Error> {
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
				Some(vec!["-lxcm=trace"])
			}
		}
	};
}

/// The Asset Hub enables the management of fungible and non-fungible assets across the network.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/asset-hub/> for more details.
#[derive(Clone)]
pub struct AssetHub(Parachain);
impl AssetHub {
	/// A new instance of the Asset Hub.
	///
	/// # Arguments
	/// * `id` - The parachain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Parachain::new("asset-hub", id, format!("asset-hub-{}", relay.chain())))
	}
}
impl_system_parachain!(AssetHub);

/// The Bridge Hub facilitates trustless interactions between Polkadot, Kusama, Ethereum, and
/// other blockchain ecosystems.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/bridge-hub/> for more details.
#[derive(Clone)]
pub struct BridgeHub(Parachain);
impl BridgeHub {
	/// A new instance of the Bridge Hub.
	///
	/// # Arguments
	/// * `id` - The parachain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Parachain::new("bridge-hub", id, format!("bridge-hub-{}", relay.chain())))
	}
}
impl_system_parachain!(BridgeHub);

/// The Collectives chain operates as a dedicated parachain exclusive to the Polkadot network.
/// This specialized infrastructure provides a foundation for various on-chain governance groups
/// essential to Polkadot's ecosystem.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/collectives/> for more details.
#[derive(Clone)]
pub struct Collectives(Parachain);
impl Collectives {
	/// A new instance of the Collectives chain.
	///
	/// # Arguments
	/// * `id` - The parachain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Parachain::new("coretime", id, format!("coretime-{}", relay.chain())))
	}
}
impl_system_parachain!(Collectives);

/// The Coretime system chain facilitates the allocation, procurement, sale, and scheduling of
/// bulk coretime, enabling tasks (such as parachains) to utilize the computation and security
/// provided by Polkadot.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/coretime/> for more details.
#[derive(Clone)]
pub struct Coretime(Parachain);
impl Coretime {
	/// A new instance of the Coretime chain.
	///
	/// # Arguments
	/// * `id` - The parachain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Parachain::new("coretime", id, format!("coretime-{}", relay.chain())))
	}
}
impl_system_parachain!(Coretime);

/// The People system chain is a specialized parachain within the Polkadot ecosystem dedicated
/// to secure, decentralized identity management.
///
/// See <https://docs.polkadot.com/polkadot-protocol/architecture/system-chains/people/> for more details.
#[derive(Clone)]
pub struct People(Parachain);
impl People {
	/// A new instance of the People chain.
	///
	/// # Arguments
	/// * `id` - The parachain identifier.
	/// * `relay` - The relay chain.
	pub fn new(id: Id, relay: Relay) -> Self {
		Self(Parachain::new("people", id, format!("people-{}", relay.chain())))
	}
}
impl_system_parachain!(People);
