// SPDX-License-Identifier: GPL-3.0
use std::path::Path;

use askama::Template;

use crate::utils::helpers::write_to_file;

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

// todo : generate directory structure
// todo : This is only for development
#[allow(unused)]
pub fn generate() {
	let cs = ChainSpec {
		token_symbol: "DOT".to_owned(),
		decimals: 10,
		initial_endowment: "1u64 << 15".to_owned(),
	};
	let rendered = cs.render().unwrap();
	let _ = write_to_file(Path::new("src/x.rs"), &rendered);
}
