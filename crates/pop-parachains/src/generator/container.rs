// SPDX-License-Identifier: GPL-3.0

use askama::Template;

#[derive(Template)]
#[template(path = "container/network.templ", escape = "none")]
pub(crate) struct Network {
	pub(crate) node: String,
}

#[derive(Template)]
#[template(path = "container/Cargo.templ", escape = "none")]
pub(crate) struct Cargo {}
