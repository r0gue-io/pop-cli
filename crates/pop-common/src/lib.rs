// SPDX-License-Identifier: GPL-3.0

#![doc = include_str!("../README.md")]

pub use account_id::{parse_account, parse_h160_account};
#[cfg(feature = "integration-tests")]
#[allow(deprecated)]
use assert_cmd::cargo::cargo_bin;
pub use build::Profile;
pub use docker::Docker;
pub use errors::Error;
pub use git::{Git, GitHub, Release};
pub use helpers::{
	get_project_name_from_path, get_relative_or_absolute_path, is_root, replace_in_file,
};
pub use metadata::format_type;
pub use signer::create_signer;
pub use sourcing::set_executable_permission;
use std::{cmp::Ordering, net::TcpListener, ops::Deref};
#[cfg(feature = "integration-tests")]
use std::{ffi::OsStr, path::Path};
pub use subxt::{Config, PolkadotConfig as DefaultConfig};
pub use subxt_signer::sr25519::Keypair;
pub use templates::{
	extractor::extract_template_files,
	frontend::{FrontendTemplate, FrontendType},
};
pub use test::test_project;

/// Module for parsing and handling account IDs.
pub mod account_id;
/// Provides functionality for accessing rate-limited APIs.
pub(crate) mod api;
/// Provides build profiles for usage when building Rust projects.
pub mod build;
/// Test utilities for mocking commands.
#[cfg(test)]
pub mod command_mock;
/// Provides utils to work with docker
pub mod docker;
/// Represents the various errors that can occur in the crate.
pub mod errors;
/// Provides functionality for interacting with Git, GitHub, repositories and releases.
pub mod git;
/// Provides general purpose file and path helpers.
pub mod helpers;
/// Provides functionality for resolving and managing Cargo manifests.
pub mod manifest;
/// Provides functionality for formatting and resolving metadata types.
pub mod metadata;
/// Provides parsers for determining Polkadot SDK versions.
pub mod polkadot_sdk;
/// Provides functionality for creating a signer from a secret URI.
pub mod signer;
/// Provides functionality for sourcing binaries from a variety of different sources.
pub mod sourcing;
/// Provides traits and functions used for templates and template extraction.
pub mod templates;
/// Module for testing utilities and functionality.
pub mod test;
/// Contains utilities for setting up a local test environment.
pub mod test_env;

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
		"aarch64" => {
			return match OS {
				"macos" => Ok("aarch64-apple-darwin"),
				_ => Ok("aarch64-unknown-linux-gnu"),
			};
		},
		"x86_64" | "x86" => {
			return match OS {
				"macos" => Ok("x86_64-apple-darwin"),
				_ => Ok("x86_64-unknown-linux-gnu"),
			};
		},
		&_ => {},
	}
	Err(Error::UnsupportedPlatform { arch: ARCH, os: OS })
}

/// Creates a new Command instance for running the `pop` binary in integration tests.
///
/// # Arguments
///
/// * `dir` - The working directory where the command will be executed.
/// * `args` - An iterator of arguments to pass to the command.
///
/// # Returns
///
/// A new Command instance configured to run the pop binary with the specified arguments
#[cfg(feature = "integration-tests")]
pub fn pop(
	dir: &Path,
	args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> tokio::process::Command {
	#[allow(deprecated)]
	let mut command = tokio::process::Command::new(cargo_bin("pop"));
	command.current_dir(dir).args(args);
	println!("{command:?}");
	command
}

/// Checks if preferred port is available, otherwise returns a random available port.
pub fn find_free_port(preferred_port: Option<u16>) -> u16 {
	// Try to bind to preferred port if provided.
	if let Some(port) = preferred_port &&
		TcpListener::bind(format!("127.0.0.1:{}", port)).is_ok()
	{
		return port;
	}

	// Else, fallback to a random available port
	TcpListener::bind("127.0.0.1:0")
		.expect("Failed to bind to an available port")
		.local_addr()
		.expect("Failed to retrieve local address. This should never occur.")
		.port()
}

/// A slice of `T` items which have been sorted.
pub struct SortedSlice<'a, T>(&'a mut [T]);
impl<'a, T> SortedSlice<'a, T> {
	/// Sorts a slice with a comparison function, preserving the initial order of equal elements.
	///
	/// # Arguments
	/// * `slice`: A mutable slice of `T` items.
	/// * `f`: A comparison function which returns an [Ordering].
	pub fn by(slice: &'a mut [T], f: impl FnMut(&T, &T) -> Ordering) -> Self {
		slice.sort_by(f);
		Self(slice)
	}

	/// Sorts a slice with a key extraction function, preserving the initial order of equal
	/// elements.
	///
	/// # Arguments
	/// * `slice`: A mutable slice of `T` items.
	/// * `f`: A comparison function which returns a key.
	pub fn by_key<K: Ord>(slice: &'a mut [T], f: impl FnMut(&T) -> K) -> Self {
		slice.sort_by_key(f);
		Self(slice)
	}
}

impl<T> Deref for SortedSlice<'_, T> {
	type Target = [T];

	fn deref(&self) -> &Self::Target {
		&self.0[..]
	}
}

/// Provides functionality for making calls to parachains or smart contracts.
pub mod call {
	// Note: cargo contract logic is used for parsing events after calling a chain. This could be
	// refactored in the future so that we don't have to use cargo contract code in
	// `pop-chains`.
	pub use contract_build::Verbosity;
	pub use contract_extrinsics::{DisplayEvents, TokenMetadata};
	pub use ink_env::DefaultEnvironment;
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[test]
	fn target_works() -> Result<()> {
		crate::command_mock::CommandMock::default().execute_sync(|| {
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
		})
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

	#[test]
	fn find_free_port_skips_busy_preferred_port() -> Result<()> {
		let listener = TcpListener::bind("127.0.0.1:0")?;
		let busy_port = listener.local_addr()?.port();
		let port = find_free_port(Some(busy_port));
		assert_ne!(port, busy_port);
		Ok(())
	}

	#[test]
	fn sorted_slice_sorts_by_function() {
		let mut values = ["one", "two", "three"];
		let sorted = SortedSlice::by(values.as_mut_slice(), |a, b| a.cmp(b));
		assert_eq!(*sorted, ["one", "three", "two"]);
	}

	#[test]
	fn sorted_slice_sorts_by_key() {
		let mut values = ['c', 'b', 'a'];
		let sorted = SortedSlice::by_key(values.as_mut_slice(), |v| *v as u8);
		assert_eq!(*sorted, ['a', 'b', 'c']);
	}
}
