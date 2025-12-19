// SPDX-License-Identifier: GPL-3.0

#![doc = include_str!("../README.md")]

mod build;
mod call;
mod errors;
mod new;
mod node;
mod templates;
mod test;
mod testing;
mod up;
mod utils;
mod verify;

pub use build::{
	BuildMode, ComposeBuildArgs, ImageVariant, Verbosity, build_smart_contract, is_supported,
};
pub use call::{
	CallOpts, call_smart_contract, call_smart_contract_from_signed_payload, dry_run_call,
	dry_run_gas_estimate_call, get_call_payload, set_up_call,
};
pub use errors::Error;
pub use new::{create_smart_contract, is_valid_contract_name};
pub use node::{
	eth_rpc_generator, ink_node_generator, is_chain_alive, run_eth_rpc_node, run_ink_node,
};
pub use templates::Contract;
pub use test::test_e2e_smart_contract;
pub use testing::{mock_build_process, new_environment};
pub use up::{
	ContractInfo, UpOpts, dry_run_gas_estimate_instantiate, dry_run_upload, get_contract_code,
	instantiate_contract_signed, instantiate_smart_contract, set_up_deployment, set_up_upload,
	submit_signed_payload, upload_contract_signed, upload_smart_contract,
};
pub use utils::{
	metadata::{
		ContractCallable, ContractFunction, ContractStorage, FunctionType, Param, extract_function,
		fetch_contract_storage, fetch_contract_storage_with_param, get_contract_storage_info,
		get_message, get_messages,
	},
	parse_hex_bytes,
};
pub use verify::{DeployedContract, VerifyContract};
// External exports
pub use contract_build::{BuildInfo, MetadataArtifacts, MetadataSpec};
pub use contract_extrinsics::{CallExec, ExtrinsicOpts, UploadCode};
pub use contract_metadata::{CodeHash, ContractMetadata};
pub use ink_env::{DefaultEnvironment, Environment};
pub use sp_core::Bytes;
pub use sp_weights::Weight;
pub use up::{get_instantiate_payload, get_upload_payload};
pub use utils::map_account::AccountMapper;

use cargo_toml::Dependency;
use once_cell::sync::Lazy;
use semver::Version;
use std::path::PathBuf;

const FALLBACK_CARGO_CONTRACT_VERSION: &str = "6.0.0-beta.1";

/// The version of `cargo-contract` used by pop-cli.
///
/// This constant attempts to extract the version from the `contract-build` dependency
/// in the workspace's `Cargo.toml` manifest at compile time. The version is used for
/// features like contract verification to ensure compatibility between the version used
/// to build contracts and the version that compiled them.
///
/// # Fallback Behavior
///
/// If the version cannot be extracted from the workspace manifest (for example, when using
/// a git commit instead of a released version like `rev = "79c4d3d"`), it falls back to
/// [`FALLBACK_CARGO_CONTRACT_VERSION`]. This fallback ensures compilation succeeds even
/// during development when using unreleased commits of `cargo-contract`.
///
/// # Important
///
/// When the workspace upgrades to a stable release version (e.g., from `6.0.0-beta.1` to `6.0.0`),
/// [`FALLBACK_CARGO_CONTRACT_VERSION`] should be updated to match. A test exists to verify this
/// and will fail if a stable version is detected that doesn't match the fallback.
pub(crate) static CARGO_CONTRACT_VERSION: Lazy<Version> = Lazy::new(|| {
	let maybe_workspace_manifest: Option<PathBuf> = || -> Option<PathBuf> {
		let current_dir = std::env::current_dir().ok()?;
		rustilities::manifest::find_workspace_manifest(current_dir)
	}();

	get_used_cargo_contract_version(maybe_workspace_manifest)
});

fn get_used_cargo_contract_version(manifest_path: Option<PathBuf>) -> Version {
	let fallback_version = Version::parse(FALLBACK_CARGO_CONTRACT_VERSION)
		.expect("The fallback version is always valid; qed;");

	let manifest = match manifest_path {
		Some(path) => match pop_common::manifest::from_path(&path) {
			Ok(manifest) => manifest,
			_ => return fallback_version,
		},
		_ => return fallback_version,
	};

	let cargo_contract_version = manifest
		.workspace
		.and_then(|workspace| {
			if let Some(contract_build_dep) = workspace.dependencies.get("contract-build") {
				match contract_build_dep {
					Dependency::Simple(version) => Some(version.clone()),
					Dependency::Detailed(detailed) if detailed.version.as_ref().is_some() => Some(
						detailed
							.version
							.as_ref()
							.expect("The match guard protects us; qed")
							.clone(),
					),
					_ => None,
				}
			} else {
				None
			}
		})
		.map(|version| Version::parse(&version));

	match cargo_contract_version {
		Some(Ok(version)) => version,
		_ => fallback_version,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	struct TestBuilder {
		temp_dir: TempDir,
	}

	impl Default for TestBuilder {
		fn default() -> Self {
			Self { temp_dir: tempfile::tempdir().unwrap() }
		}
	}

	impl TestBuilder {
		fn with_manifest_containing_cargo_contract(self) -> Self {
			let manifest_path = self.temp_dir.path().join("Cargo.toml");
			let contents = r#"[workspace]
members = ["crate1", "crate2"]

[workspace.dependencies]
contract-build = "0.1.0"
"#;
			std::fs::write(&manifest_path, contents).unwrap();
			self
		}

		fn with_manifest_containing_cargo_contract_with_complex_declaration(self) -> Self {
			let manifest_path = self.temp_dir.path().join("Cargo.toml");
			let contents = r#"[workspace]
members = ["crate1", "crate2"]

[workspace.dependencies]
contract-build = { version = "0.1.0", default-features = false }
"#;
			std::fs::write(&manifest_path, contents).unwrap();
			self
		}

		fn with_manifest_not_containing_cargo_contract(self) -> Self {
			let manifest_path = self.temp_dir.path().join("Cargo.toml");
			let contents = r#"[workspace]
members = ["crate1", "crate2"]
"#;
			std::fs::write(&manifest_path, contents).unwrap();
			self
		}

		fn with_invalid_manifest(self) -> Self {
			let manifest_path = self.temp_dir.path().join("Cargo.toml");
			std::fs::write(&manifest_path, "test").unwrap();
			self
		}

		fn execute<Test>(self, test: Test)
		where
			Test: FnOnce(Self),
		{
			test(self)
		}
	}

	#[test]
	fn get_used_cargo_contract_version_works_with_simple_versions() {
		TestBuilder::default()
			.with_manifest_containing_cargo_contract()
			.execute(|builder| {
				assert_eq!(
					Version::parse("0.1.0").unwrap(),
					get_used_cargo_contract_version(Some(
						builder.temp_dir.path().join("Cargo.toml")
					))
				);
			});
	}

	#[test]
	fn get_used_cargo_contract_version_works_with_complex_version() {
		TestBuilder::default()
			.with_manifest_containing_cargo_contract_with_complex_declaration()
			.execute(|builder| {
				assert_eq!(
					Version::parse("0.1.0").unwrap(),
					get_used_cargo_contract_version(Some(
						builder.temp_dir.path().join("Cargo.toml")
					))
				);
			});
	}

	#[test]
	fn get_used_cargo_contract_version_returns_fallback_if_contract_build_not_present() {
		TestBuilder::default()
			.with_manifest_not_containing_cargo_contract()
			.execute(|builder| {
				assert_eq!(
					Version::parse(FALLBACK_CARGO_CONTRACT_VERSION).unwrap(),
					get_used_cargo_contract_version(Some(
						builder.temp_dir.path().join("Cargo.toml")
					))
				);
			});
	}

	#[test]
	fn get_used_cargo_contract_version_returns_fallback_if_invalid_manifest() {
		TestBuilder::default().with_invalid_manifest().execute(|builder| {
			assert_eq!(
				Version::parse(FALLBACK_CARGO_CONTRACT_VERSION).unwrap(),
				get_used_cargo_contract_version(Some(builder.temp_dir.path().join("Cargo.toml")))
			);
		});
	}

	#[test]
	fn get_used_cargo_contract_version_returns_fallback_if_none() {
		TestBuilder::default().execute(|_builder| {
			assert_eq!(
				Version::parse(FALLBACK_CARGO_CONTRACT_VERSION).unwrap(),
				get_used_cargo_contract_version(None)
			);
		});
	}

	/// Test to ensure the fallback version is kept up to date with the workspace version.
	///
	/// This test verifies that when the workspace uses a stable version of `contract-build`
	/// (i.e., a version without prerelease identifiers like `-beta`, `-rc`, etc.), the
	/// fallback constant matches it.
	///
	/// # Why this test exists
	///
	/// When developing, we often use git commits of `cargo-contract` which can't be parsed
	/// as versions, so the fallback is used. When we upgrade to a stable release (e.g., from
	/// `6.0.0-beta.1` to `6.0.0`), this test will fail to remind us to update
	/// `FALLBACK_CARGO_CONTRACT_VERSION`.
	///
	/// # When this test fails
	///
	/// - The workspace uses a new version (e.g., `6.0.0`) that differs from the fallback
	/// - This indicates `FALLBACK_CARGO_CONTRACT_VERSION` needs to be updated
	#[test]
	fn fallback_version_is_up_to_date_with_workspace() {
		let workspace_manifest = std::env::current_dir()
			.ok()
			.and_then(|dir| rustilities::manifest::find_workspace_manifest(dir));

		let computed_version = get_used_cargo_contract_version(workspace_manifest);
		let fallback_version =
			Version::parse(FALLBACK_CARGO_CONTRACT_VERSION).expect("Fallback version is valid");

		// If the computed version is different from the fallback
		if computed_version != fallback_version {
			panic!(
				"The workspace is using a version of contract-build ({}) that differs \
				 from FALLBACK_CARGO_CONTRACT_VERSION ({}).\n\
				 Please update FALLBACK_CARGO_CONTRACT_VERSION to \"{}\" in lib.rs",
				computed_version, fallback_version, computed_version
			);
		}
	}
}
