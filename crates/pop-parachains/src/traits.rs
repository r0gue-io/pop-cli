// SPDX-License-Identifier: GPL-3.0

/// A communication endpoint.
pub type Port = u16;
/// A parachain identifier.
pub type Id = u32;

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
pub trait Parachain {
	/// The parachain identifier.
	fn id(&self) -> Id;

	/// The name of the chain.
	fn name(&self) -> &str;

	/// Set the parachain identifier.
	///
	/// # Arguments
	/// * `id` - The parachain identifier.
	fn set_id(&mut self, id: Id);
}
