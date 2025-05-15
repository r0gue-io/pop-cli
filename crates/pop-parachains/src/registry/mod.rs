// SPDX-License-Identifier: GPL-3.0

use crate::{
	traits::{
		Args, Binary, ChainSpec, GenesisOverrides, Id, Node, Override, Port, Rollup as RollupT,
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
macro_rules! impl_rollup {
	($name:ty) => {
		impl AsRollup for $name {
			fn as_rollup(&self) -> &Rollup {
				&self.0
			}
			fn as_rollup_mut(&mut self) -> &mut Rollup {
				&mut self.0
			}
		}

		impl traits::Rollup for $name {
			fn as_any(&self) -> &dyn std::any::Any {
				self
			}
		}
	};
}

mod pop;
mod system;

pub(crate) type RollupTypeId = TypeId;
type Registry = HashMap<Relay, Vec<Box<dyn traits::Rollup>>>;

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
			BridgeHub::new(1_002, Paseo).into(),
			Collectives::new(1_001, Paseo).into(),
			Coretime::new(1_004, Paseo).into(),
			People::new(1_005, Paseo).into(),
			// Others
			Pop::new(4_001, "pop-devnet-local").into(),
		],
	);
	registry.insert(
		Polkadot,
		vec![
			// System chains
			AssetHub::new(1_000, Polkadot).into(),
			BridgeHub::new(1_002, Polkadot).into(),
			Collectives::new(1_001, Polkadot).into(),
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
			BridgeHub::new(1_002, Westend).into(),
			Collectives::new(1_001, Westend).into(),
			Coretime::new(1_004, Westend).into(),
			People::new(1_005, Westend).into(),
		],
	);
	registry
};

/// Returns the rollups registered for the provided relay chain.
///
/// # Arguments
/// * `relay` - The relay chain.
pub fn rollups(relay: &Relay) -> &'static [Box<dyn traits::Rollup>] {
	static REGISTRY: OnceLock<HashMap<Relay, Vec<Box<dyn traits::Rollup>>>> = OnceLock::new();
	static EMPTY: Vec<Box<dyn traits::Rollup>> = Vec::new();

	REGISTRY.get_or_init(|| REGISTRAR(HashMap::new())).get(relay).unwrap_or(&EMPTY)
}

// A base type, used by rollup implementations to reduce boilerplate code.
#[derive(Clone)]
struct Rollup {
	name: String,
	id: Id,
	chain: String,
	port: Option<Port>,
}

impl Rollup {
	fn new(name: impl Into<String>, id: Id, chain: impl Into<String>) -> Self {
		Self { name: name.into(), id, chain: chain.into(), port: None }
	}
}

impl ChainSpec for Rollup {
	fn chain(&self) -> &str {
		self.chain.as_str()
	}
}

impl RollupT for Rollup {
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

impl Node for Rollup {
	fn port(&self) -> Option<&Port> {
		self.port.as_ref()
	}

	fn set_port(&mut self, port: Port) {
		self.port = Some(port);
	}
}

trait AsRollup {
	fn as_rollup(&self) -> &Rollup;
	fn as_rollup_mut(&mut self) -> &mut Rollup;
}

impl<T: AsRollup + 'static> ChainSpec for T {
	fn chain(&self) -> &str {
		self.as_rollup().chain()
	}
}

impl<T: AsRollup + 'static> RollupT for T {
	fn id(&self) -> Id {
		self.as_rollup().id()
	}

	fn name(&self) -> &str {
		self.as_rollup().name()
	}

	fn set_id(&mut self, id: Id) {
		self.as_rollup_mut().set_id(id);
	}
}

impl<T: AsRollup + 'static> Node for T {
	fn port(&self) -> Option<&Port> {
		self.as_rollup().port()
	}

	fn set_port(&mut self, port: Port) {
		self.as_rollup_mut().set_port(port);
	}
}

impl<T: traits::Rollup> From<T> for Box<dyn traits::Rollup> {
	fn from(value: T) -> Self {
		Box::new(value)
	}
}

/// Traits used by the rollup registry.
pub mod traits {
	use super::*;

	/// A meta-trait used specifically for trait objects.
	pub trait Rollup:
		Any
		+ Args
		+ Binary
		+ ChainSpec
		+ GenesisOverrides
		+ Node
		+ Requires
		+ crate::traits::Rollup
		+ RollupClone
		+ Send
		+ Source
		+ Sync
	{
		/// Allows casting to [`Any`].
		fn as_any(&self) -> &dyn Any;
	}

	/// A helper trait for ensuring [Rollup] trait objects can be cloned.
	pub trait RollupClone {
		/// Returns a copy of the value.
		fn clone_box(&self) -> Box<dyn Rollup>;
	}

	impl<T: 'static + Rollup + Clone> RollupClone for T {
		fn clone_box(&self) -> Box<dyn Rollup> {
			Box::new(self.clone())
		}
	}

	impl Clone for Box<dyn Rollup> {
		fn clone(&self) -> Self {
			RollupClone::clone_box(self.as_ref())
		}
	}

	/// The requirements of a rollup.
	pub trait Requires {
		/// Defines the requirements of a rollup, namely which other rollup it depends on and any
		/// corresponding overrides to genesis state.
		fn requires(&self) -> Option<HashMap<RollupTypeId, Override>> {
			None
		}
	}
}

#[test]
fn check() {
	use std::any::{Any, TypeId};
	use traits::Rollup;

	let asset_hub = AssetHub::new(1_000, Relay::Paseo);
	let bridge_hub = BridgeHub::new(1_002, Relay::Polkadot);

	assert_ne!(asset_hub.type_id(), bridge_hub.type_id());

	let asset_hub: Box<dyn Rollup> = Box::new(asset_hub);
	let bridge_hub: Box<dyn Rollup> = Box::new(bridge_hub);

	assert_ne!(asset_hub.as_any().type_id(), bridge_hub.as_any().type_id());

	assert_eq!(asset_hub.as_any().type_id(), TypeId::of::<AssetHub>());
	assert_eq!(bridge_hub.as_any().type_id(), TypeId::of::<BridgeHub>());
}
