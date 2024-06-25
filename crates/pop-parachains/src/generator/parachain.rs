// SPDX-License-Identifier: GPL-3.0

use askama::Template;

#[derive(Template)]
#[template(path = "base/chain_spec.templ", escape = "none")]
pub(crate) struct ChainSpec {
	pub(crate) token_symbol: String,
	pub(crate) decimals: u8,
	pub(crate) initial_endowment: String,
}

#[derive(Template)]
#[template(path = "base/network.templ", escape = "none")]
pub(crate) struct Network {
	pub(crate) node: String,
}
