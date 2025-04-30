// SPDX-License-Identifier: GPL-3.0

use crate::{
	traits::{Args, Binary, ChainSpec, Id, Node, Parachain as ParachainT, Port},
	up::Relay,
};
pub use pop::*;
use pop_common::sourcing::traits::Source;
use serde_json::{Map, Value};
use std::{
	any::{Any, TypeId},
	collections::HashMap,
	sync::OnceLock,
};
pub use system::*;

// Macro for reducing boilerplate code.
macro_rules! impl_parachain {
	($name:ty) => {
		impl AsPara for $name {
			fn as_para(&self) -> &Parachain {
				&self.0
			}
			fn as_para_mut(&mut self) -> &mut Parachain {
				&mut self.0
			}
		}

		impl Para for $name {
			fn as_any(&self) -> &dyn std::any::Any {
				self
			}
		}
	};
}

mod pop;
mod system;

pub(crate) type Override = Box<dyn FnMut(&mut Map<String, Value>)>;
pub(crate) type ParaTypeId = TypeId;
type Registry = HashMap<Relay, Vec<Box<dyn Para>>>;

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
	registry
};

/// Returns the parachains registered for the provided relay chain.
///
/// # Arguments
/// * `relay` - The relay chain.
pub fn parachains(relay: &Relay) -> &'static [Box<dyn Para>] {
	static REGISTRY: OnceLock<HashMap<Relay, Vec<Box<dyn Para>>>> = OnceLock::new();
	static EMPTY: Vec<Box<dyn Para>> = Vec::new();

	REGISTRY.get_or_init(|| REGISTRAR(HashMap::new())).get(relay).unwrap_or(&EMPTY)
}

/// A meta-trait used specifically for trait objects.
pub trait Para:
	Any + Args + Binary + ChainSpec + ParachainT + ParaClone + Node + Requires + Send + Source + Sync
{
	/// Allows casting to [`Any`].
	fn as_any(&self) -> &dyn Any;
}

/// A helper trait for ensuring [Para] trait objects can be cloned.
pub trait ParaClone {
	/// Returns a copy of the value.
	fn clone_box(&self) -> Box<dyn Para>;
}

impl<T: 'static + Para + Clone> ParaClone for T {
	fn clone_box(&self) -> Box<dyn Para> {
		Box::new(self.clone())
	}
}

impl Clone for Box<dyn Para> {
	fn clone(&self) -> Self {
		ParaClone::clone_box(self.as_ref())
	}
}

/// The requirements of a parachain.
pub trait Requires {
	/// Defines the requirements of a parachain, namely which other chains it depends on and any
	/// corresponding overrides to genesis state.
	fn requires(&self) -> Option<HashMap<ParaTypeId, Override>> {
		None
	}
}

// A base type, used by parachain implementations to reduce boilerplate code.
#[derive(Clone)]
struct Parachain {
	name: String,
	id: Id,
	chain: String,
	port: Option<Port>,
}

impl Parachain {
	fn new(name: impl Into<String>, id: Id, chain: impl Into<String>) -> Self {
		Self { name: name.into(), id, chain: chain.into(), port: None }
	}
}

impl ChainSpec for Parachain {
	fn chain(&self) -> &str {
		self.chain.as_str()
	}
}

impl ParachainT for Parachain {
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

impl Node for Parachain {
	fn port(&self) -> Option<&Port> {
		self.port.as_ref()
	}

	fn set_port(&mut self, port: Port) {
		self.port = Some(port);
	}
}

trait AsPara {
	fn as_para(&self) -> &Parachain;
	fn as_para_mut(&mut self) -> &mut Parachain;
}

impl<T: AsPara + 'static> ChainSpec for T {
	fn chain(&self) -> &str {
		self.as_para().chain()
	}
}

impl<T: AsPara + 'static> ParachainT for T {
	fn id(&self) -> Id {
		self.as_para().id()
	}

	fn name(&self) -> &str {
		self.as_para().name()
	}

	fn set_id(&mut self, id: Id) {
		self.as_para_mut().set_id(id);
	}
}

impl<T: AsPara + 'static> Node for T {
	fn port(&self) -> Option<&Port> {
		self.as_para().port()
	}

	fn set_port(&mut self, port: Port) {
		self.as_para_mut().set_port(port);
	}
}

impl<T: Para> From<T> for Box<dyn Para> {
	fn from(value: T) -> Self {
		Box::new(value)
	}
}

#[test]
fn check() {
	use std::any::{Any, TypeId};

	let asset_hub = AssetHub::new(1_000, Relay::Paseo);
	let bridge_hub = BridgeHub::new(1_002, Relay::Polkadot);

	assert_ne!(asset_hub.type_id(), bridge_hub.type_id());

	let asset_hub: Box<dyn Para> = Box::new(asset_hub);
	let bridge_hub: Box<dyn Para> = Box::new(bridge_hub);

	assert_ne!(asset_hub.as_any().type_id(), bridge_hub.as_any().type_id());

	assert_eq!(asset_hub.as_any().type_id(), TypeId::of::<AssetHub>());
	assert_eq!(bridge_hub.as_any().type_id(), TypeId::of::<BridgeHub>());
}
