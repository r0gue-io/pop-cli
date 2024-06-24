// SPDX-License-Identifier: GPL-3.0
#![doc = include_str!("../README.md")]
mod build;
mod errors;
mod generator;
mod new_pallet;
mod new_parachain;
mod templates;
mod up;
mod utils;

pub use build::{build_parachain, generate_chain_spec, node_release_path};
pub use errors::Error;
pub use indexmap::IndexSet;
pub use new_pallet::{create_pallet_template, TemplatePalletConfig};
pub use new_parachain::instantiate_template_dir;
pub use templates::{Config, Provider, Template};
pub use up::{Binary, Status, Zombienet};
pub use utils::git::{Git, GitHub, Release};
pub use utils::helpers::is_initial_endowment_valid;
pub use utils::pallet_helpers::resolve_pallet_path;
/// Information about the Node. External export from Zombienet-SDK.
pub use zombienet_sdk::NetworkNode;

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
