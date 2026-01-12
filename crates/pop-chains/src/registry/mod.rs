// SPDX-License-Identifier: GPL-3.0

use crate::{
	traits::{
		Args, Binary, Chain as ChainT, ChainSpec, GenesisOverrides, Id, Node, Override, Port,
	},
	up::Relay,
};
pub use pop::*;
use pop_common::sourcing::traits::Source;
use std::{
	any::{Any, TypeId},
	collections::HashMap,
	sync::OnceLock,
};
pub use system::*;

// Macro for reducing boilerplate code.
macro_rules! impl_chain {
	($name:ty) => {
		impl AsChain for $name {
			fn as_chain(&self) -> &Chain {
				&self.0
			}

			fn as_chain_mut(&mut self) -> &mut Chain {
				&mut self.0
			}
		}

		impl traits::Chain for $name {
			fn as_any(&self) -> &dyn std::any::Any {
				self
			}
		}
	};
}

mod pop;
mod system;

pub(crate) type ChainTypeId = TypeId;
type Registry = HashMap<Relay, Vec<Box<dyn traits::Chain>>>;

const REGISTRAR: fn(Registry) -> Registry = |mut registry| {
	use Relay::*;
	registry.insert(
		Kusama,
		vec![
			// System chains
			AssetHub::new(1_000, Kusama).into(),
			BridgeHub::new(1_002, Kusama).into(),
			Coretime::new(1_004, Kusama).into(),
			People::new(1_005, Kusama).into(),
		],
	);
	registry.insert(
		Paseo,
		vec![
			// System chains
			AssetHub::new(1_000, Paseo).into(),
			Collectives::new(1_001, Paseo).into(),
			BridgeHub::new(1_002, Paseo).into(),
			Coretime::new(1_004, Paseo).into(),
			People::new(1_005, Paseo).into(),
			PassetHub::new(1_111, Paseo).into(),
			// Others
			Pop::new(4_001, "pop-devnet-local").into(),
		],
	);
	registry.insert(
		Polkadot,
		vec![
			// System chains
			AssetHub::new(1_000, Polkadot).into(),
			Collectives::new(1_001, Polkadot).into(),
			BridgeHub::new(1_002, Polkadot).into(),
			Coretime::new(1_004, Polkadot).into(),
			People::new(1_005, Polkadot).into(),
			// Others
			Pop::new(3_395, "pop-local").into(),
		],
	);
	registry.insert(
		Westend,
		vec![
			// System chains
			AssetHub::new(1_000, Westend).into(),
			Collectives::new(1_001, Westend).into(),
			BridgeHub::new(1_002, Westend).into(),
			Coretime::new(1_004, Westend).into(),
			People::new(1_005, Westend).into(),
		],
	);
	registry
};

/// Returns the chains registered for the provided relay chain.
///
/// # Arguments
/// * `relay` - The relay chain.
pub fn chains(relay: &Relay) -> &'static [Box<dyn traits::Chain>] {
	static REGISTRY: OnceLock<HashMap<Relay, Vec<Box<dyn traits::Chain>>>> = OnceLock::new();
	static EMPTY: Vec<Box<dyn traits::Chain>> = Vec::new();

	REGISTRY.get_or_init(|| REGISTRAR(HashMap::new())).get(relay).unwrap_or(&EMPTY)
}

// A base type, used by chain implementations to reduce boilerplate code.
#[derive(Clone, Debug, PartialEq)]
struct Chain {
	name: String,
	id: Id,
	chain: String,
	port: Option<Port>,
}

impl Chain {
	fn new(name: impl Into<String>, id: Id, chain: impl Into<String>) -> Self {
		Self { name: name.into(), id, chain: chain.into(), port: None }
	}
}

impl ChainSpec for Chain {
	fn chain(&self) -> &str {
		self.chain.as_str()
	}
}

impl ChainT for Chain {
	fn id(&self) -> Id {
		self.id
	}

	fn name(&self) -> &str {
		self.name.as_str()
	}

	fn set_id(&mut self, id: Id) {
		self.id = id;
	}
}

impl Node for Chain {
	fn port(&self) -> Option<&Port> {
		self.port.as_ref()
	}

	fn set_port(&mut self, port: Port) {
		self.port = Some(port);
	}
}

trait AsChain {
	fn as_chain(&self) -> &Chain;
	fn as_chain_mut(&mut self) -> &mut Chain;
}

impl<T: AsChain + 'static> ChainSpec for T {
	fn chain(&self) -> &str {
		self.as_chain().chain()
	}
}

impl<T: AsChain + 'static> ChainT for T {
	fn id(&self) -> Id {
		self.as_chain().id()
	}

	fn name(&self) -> &str {
		self.as_chain().name()
	}

	fn set_id(&mut self, id: Id) {
		self.as_chain_mut().set_id(id);
	}
}

impl<T: AsChain + 'static> Node for T {
	fn port(&self) -> Option<&Port> {
		self.as_chain().port()
	}

	fn set_port(&mut self, port: Port) {
		self.as_chain_mut().set_port(port);
	}
}

impl<T: traits::Chain> From<T> for Box<dyn traits::Chain> {
	fn from(value: T) -> Self {
		Box::new(value)
	}
}

/// Traits used by the chain registry.
pub mod traits {
	use super::*;

	/// A meta-trait used specifically for trait objects.
	pub trait Chain:
		Any
		+ Args
		+ Binary
		+ ChainSpec
		+ GenesisOverrides
		+ Node
		+ Requires
		+ crate::traits::Chain
		+ ChainClone
		+ Send
		+ Source<Error = crate::Error>
		+ Sync
	{
		/// Allows casting to [`Any`].
		fn as_any(&self) -> &dyn Any;
	}

	/// A helper trait for ensuring [Chain] trait objects can be cloned.
	pub trait ChainClone {
		/// Returns a copy of the value.
		fn clone_box(&self) -> Box<dyn Chain>;
	}

	impl<T: 'static + Chain + Clone> ChainClone for T {
		fn clone_box(&self) -> Box<dyn Chain> {
			Box::new(self.clone())
		}
	}

	impl Clone for Box<dyn Chain> {
		fn clone(&self) -> Self {
			ChainClone::clone_box(self.as_ref())
		}
	}

	/// The requirements of a chain.
	pub trait Requires {
		/// Defines the requirements of a chain, namely which other chain it depends on and any
		/// corresponding overrides to genesis state.
		fn requires(&self) -> Option<HashMap<ChainTypeId, Override>> {
			None
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::registry::traits::{Chain as _, Requires};
	use Relay::*;

	#[test]
	fn chain_works() {
		let mut chain = Chain::new("test-chain", 2_000, "test-local");
		assert_eq!(chain.name(), "test-chain");
		assert_eq!(chain.id(), 2_000);
		assert_eq!(chain.chain(), "test-local");
		assert!(chain.port().is_none());

		chain.set_id(2_002);
		assert_eq!(chain.id(), 2_002);

		chain.set_port(9_944);
		assert_eq!(chain.port().unwrap(), &9_944u16);
	}

	#[test]
	fn impl_chain_works() {
		let mut asset_hub = AssetHub::new(1_000, Paseo);
		asset_hub.as_chain_mut().id = 1;
		assert_eq!(asset_hub.id(), 1);
		assert_eq!(asset_hub.as_chain(), &asset_hub.0);
		assert_eq!(asset_hub.as_any().type_id(), asset_hub.type_id());
		asset_hub.set_id(1_000);
		assert_eq!(asset_hub.id(), 1_000);
		asset_hub.set_port(9_944);
		assert_eq!(asset_hub.port().unwrap(), &9_944u16);
		assert!(asset_hub.requires().is_none());
	}

	#[test]
	fn clone_works() {
		let asset_hub: Box<dyn traits::Chain> = Box::new(AssetHub::new(1_000, Paseo));
		assert_eq!(
			asset_hub.clone().as_any().downcast_ref::<AssetHub>(),
			asset_hub.as_any().downcast_ref::<AssetHub>()
		);
	}

	#[test]
	fn kusama_registry_works() {
		let registry = chains(&Kusama);
		assert!(contains::<AssetHub>(registry, 1_000));
		assert!(contains::<BridgeHub>(registry, 1_002));
		assert!(contains::<Coretime>(registry, 1_004));
		assert!(contains::<People>(registry, 1_005));
	}

	#[test]
	fn paseo_registry_works() {
		let registry = chains(&Paseo);
		assert!(contains::<AssetHub>(registry, 1_000));
		assert!(contains::<Collectives>(registry, 1_001));
		assert!(contains::<BridgeHub>(registry, 1_002));
		assert!(contains::<Coretime>(registry, 1_004));
		assert!(contains::<People>(registry, 1_005));
		assert!(contains::<PassetHub>(registry, 1_111));
		assert!(contains::<Pop>(registry, 4_001));
	}

	#[test]
	fn polkadot_registry_works() {
		let registry = chains(&Polkadot);
		assert!(contains::<AssetHub>(registry, 1_000));
		assert!(contains::<Collectives>(registry, 1_001));
		assert!(contains::<BridgeHub>(registry, 1_002));
		assert!(contains::<Coretime>(registry, 1_004));
		assert!(contains::<People>(registry, 1_005));
		assert!(contains::<Pop>(registry, 3_395));
	}

	#[test]
	fn westend_registry_works() {
		let registry = chains(&Westend);
		assert!(contains::<AssetHub>(registry, 1_000));
		assert!(contains::<Collectives>(registry, 1_001));
		assert!(contains::<BridgeHub>(registry, 1_002));
		assert!(contains::<Coretime>(registry, 1_004));
		assert!(contains::<People>(registry, 1_005));
	}

	#[test]
	fn type_checks() {
		use std::any::{Any, TypeId};
		use traits::Chain;

		let asset_hub = AssetHub::new(1_000, Paseo);
		let bridge_hub = BridgeHub::new(1_002, Polkadot);

		assert_ne!(asset_hub.type_id(), bridge_hub.type_id());

		let asset_hub: Box<dyn Chain> = Box::new(asset_hub);
		let bridge_hub: Box<dyn Chain> = Box::new(bridge_hub);

		assert_ne!(asset_hub.as_any().type_id(), bridge_hub.as_any().type_id());

		assert_eq!(asset_hub.as_any().type_id(), TypeId::of::<AssetHub>());
		assert_eq!(bridge_hub.as_any().type_id(), TypeId::of::<BridgeHub>());
	}

	fn contains<T: 'static>(registry: &[Box<dyn traits::Chain>], id: Id) -> bool {
		registry
			.iter()
			.any(|r| r.as_any().type_id() == TypeId::of::<T>() && r.id() == id)
	}
}
