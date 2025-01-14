// SPDX-License-Identifier: GPL-3.0

use std::net::TcpListener;

pub use build::Profile;
pub use errors::Error;
pub use git::{Git, GitHub, Release};
pub use helpers::{get_project_name_from_path, prefix_with_current_dir_if_needed, replace_in_file};
pub use manifest::{add_crate_to_workspace, find_workspace_toml};
pub use metadata::format_type;
pub use signer::create_signer;
pub use sourcing::set_executable_permission;
pub use subxt::{Config, PolkadotConfig as DefaultConfig};
pub use subxt_signer::sr25519::Keypair;
pub use templates::extractor::extract_template_files;

pub mod build;
pub mod errors;
pub mod git;
pub mod helpers;
pub mod manifest;
/// Provides functionality for formatting and resolving metadata types.
pub mod metadata;
pub mod polkadot_sdk;
/// Provides functionality for creating a signer from a secret URI.
pub mod signer;
pub mod sourcing;
pub mod templates;

static APP_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Trait for observing status updates.
pub trait Status {
	/// Update the observer with the provided `status`.
	fn update(&self, status: &str);
}

impl Status for () {
	// no-op: status updates are ignored
	fn update(&self, _: &str) {}
}

/// Determines the target triple based on the current platform.
pub fn target() -> Result<&'static str, Error> {
	use std::env::consts::*;

	if OS == "windows" {
		return Err(Error::UnsupportedPlatform { arch: ARCH, os: OS });
	}

	match ARCH {
		"aarch64" =>
			return match OS {
				"macos" => Ok("aarch64-apple-darwin"),
				_ => Ok("aarch64-unknown-linux-gnu"),
			},
		"x86_64" | "x86" =>
			return match OS {
				"macos" => Ok("x86_64-apple-darwin"),
				_ => Ok("x86_64-unknown-linux-gnu"),
			},
		&_ => {},
	}
	Err(Error::UnsupportedPlatform { arch: ARCH, os: OS })
}

/// Checks if preferred port is available, otherwise returns a random available port.
pub fn find_free_port(preferred_port: Option<u16>) -> u16 {
	// Try to bind to preferred port if provided.
	if let Some(port) = preferred_port {
		if TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok() {
			return port;
		}
	}

	// Else, fallback to a random available port
	TcpListener::bind("127.0.0.1:0")
		.expect("Failed to bind to an available port")
		.local_addr()
		.expect("Failed to retrieve local address. This should never occur.")
		.port()
}

/// Provides functionality for making calls to parachains or smart contracts.
pub mod call {
	// Note: cargo contract logic is used for parsing events after calling a chain. This could be
	// refactored in the future so that we don't have to use cargo contract code in
	// `pop-parachains`.
	pub use contract_build::Verbosity;
	pub use contract_extrinsics::{DisplayEvents, TokenMetadata};
	pub use ink_env::DefaultEnvironment;
}

#[cfg(test)]
mod test {
	use super::*;
	use anyhow::Result;

	#[test]
	fn target_works() -> Result<()> {
		use std::{process::Command, str};
		let output = Command::new("rustc").arg("-vV").output()?;
		let output = str::from_utf8(&output.stdout)?;
		let target_expected = output
			.lines()
			.find(|l| l.starts_with("host: "))
			.map(|l| &l[6..])
			.unwrap()
			.to_string();
		assert_eq!(target()?, target_expected);
		Ok(())
	}

	#[test]
	fn find_free_port_works() -> Result<()> {
		let port = find_free_port(None);
		let addr = format!("127.0.0.1:{}", port);
		// Constructs the TcpListener from the above port
		let listener = TcpListener::bind(&addr);
		assert!(listener.is_ok());
		Ok(())
	}
}
