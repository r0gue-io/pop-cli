// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, utils::get_manifest_path};
pub use contract_build::{BuildMode, ComposeBuildArgs, ImageVariant, MetadataSpec, Verbosity};
use contract_build::{BuildResult, ExecuteArgs, execute};
use std::path::Path;

/// Build the smart contract located at the specified `path` in `build_release` mode.
///
/// If `build_mode` is `Verifiable`, this function will call a docker image running a verifiable
/// build using some CLI (by default, `cargo contract`). The generic type `T` can be used here to
/// determine how to handle the arguments passed to the docker container, as the CLI called inside
/// might not recognize some commands passed by the user
///
/// # Arguments
/// * `path` - The optional path to the smart contract manifest, defaulting to the current directory
///   if not specified.
/// * `release` - Whether the smart contract should be built without any debugging functionality.
/// * `verbosity` - The build output verbosity.
/// * `metadata_spec` - Optionally specify the contract metadata format/version.
pub fn build_smart_contract<T: ComposeBuildArgs>(
	path: &Path,
	build_mode: BuildMode,
	verbosity: Verbosity,
	metadata_spec: Option<MetadataSpec>,
	image: Option<ImageVariant>,
) -> anyhow::Result<BuildResult> {
	let manifest_path = get_manifest_path(path)?;

	let mut args =
		ExecuteArgs { manifest_path, build_mode, verbosity, metadata_spec, ..Default::default() };

	if let Some(image) = image {
		args.image = image;
	}

	// Execute the build and log the output of the build
	match build_mode {
		// For verifiable contracts, execute calls docker_build (https://github.com/use-ink/cargo-contract/blob/master/crates/build/src/lib.rs#L595) which launches a blocking tokio runtime to handle the async operations (https://github.com/use-ink/cargo-contract/blob/master/crates/build/src/docker.rs#L135). The issue is that pop is itself a tokio runtime, launching another blocking one isn't allowed by tokio. So for verifiable contracts we need to first block the main pop tokio runtime before calling execute
		BuildMode::Verifiable => tokio::task::block_in_place(|| execute::<T>(args)),
		_ => execute::<T>(args),
	}
}

/// Determines whether the manifest at the supplied path is a supported smart contract project.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not
///   specified.
pub fn is_supported(path: &Path) -> Result<bool, Error> {
	Ok(pop_common::manifest::from_path(path)?.dependencies.contains_key("ink"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use contract_build::new_contract_project;
	use duct::cmd;

	#[test]
	fn is_supported_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(path).run()?;
		assert!(!is_supported(&path.join(name))?);

		// Contract
		let name = "flipper";
		new_contract_project(name, Some(&path), None)?;
		assert!(is_supported(&path.join(name))?);
		Ok(())
	}
}
