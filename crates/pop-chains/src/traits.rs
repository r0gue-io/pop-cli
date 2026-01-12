// SPDX-License-Identifier: GPL-3.0

use serde_json::{Map, Value};

/// A chain identifier.
pub type Id = u32;
/// A function for providing genesis overrides.
pub type Override = Box<dyn FnMut(&mut Map<String, Value>)>;
/// A communication endpoint.
pub type Port = u16;

/// The arguments used when launching a node.
pub trait Args {
	/// The default arguments to be used when launching a node.
	fn args(&self) -> Option<Vec<&str>>;
}

/// The binary used to launch a node.
pub trait Binary {
	/// The name of the binary.
	fn binary(&self) -> &'static str;
}

/// A specification of a chain, providing the genesis configurations, boot nodes, and other
/// parameters required to launch the chain.
pub trait ChainSpec {
	/// The identifier of the chain, as used by the chain specification.
	fn chain(&self) -> &str;
}

/// Any overrides to genesis state.
pub trait GenesisOverrides {
	/// Any overrides to genesis state.
	fn genesis_overrides(&self) -> Option<Override> {
		None
	}
}

/// A node.
pub trait Node {
	/// The port to be used.
	fn port(&self) -> Option<&Port>;

	/// Set the port to be used.
	///
	/// # Arguments
	/// * `port` - The port to be used.
	fn set_port(&mut self, port: Port);
}

/// An application-specific blockchain, validated by the validators of the relay chain.
pub trait Chain {
	/// The chain identifier.
	fn id(&self) -> Id;

	/// The name of the chain.
	fn name(&self) -> &str;

	/// Set the chain identifier.
	///
	/// # Arguments
	/// * `id` - The chain identifier.
	fn set_id(&mut self, id: Id);
}
