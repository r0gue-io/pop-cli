// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, utils::get_manifest_path};
pub use contract_build::{BuildMode, ComposeBuildArgs, ImageVariant, MetadataSpec, Verbosity};
use contract_build::{BuildResult, ExecuteArgs, execute};
use regex::Regex;
use std::{fs, path::Path};
use toml::Value;

pub(crate) struct PopComposeBuildArgs;
impl ComposeBuildArgs for PopComposeBuildArgs {
	fn compose_build_args() -> anyhow::Result<Vec<String>> {
		process_build_args(std::env::args())
	}
}

/// Process build arguments by filtering and transforming them for Docker builds.
fn process_build_args<I>(args: I) -> anyhow::Result<Vec<String>>
where
	I: IntoIterator<Item = String>,
{
	// Match `pop` related args in `pop build --verifiable` or `pop verify` that shouldn't be passed
	// to Docker image. `--path-pos` should also be ignored.
	let path_pos_regex = Regex::new(r#"(--path-pos)[ ]*[^ ]*[ ]*"#).expect("Valid regex; qed;");
	// If `--image` is passed in build command, remove it.
	let image_regex = Regex::new(r#"(--image)[ ]*[^ ]*[ ]*"#).expect("Valid regex; qed;");
	// If verify, we ignore `--contract-path`, `--url` and `--address`.
	let verify_regex =
		Regex::new(r#"(--contract-path|--url|--address)[ ]*[^ ]*[ ]*"#).expect("Valid regex; qed;");
	// Replace `--path <value>` with `--manifest-path <value>/Cargo.toml`.
	let path_regex = Regex::new(r#"--path\s+([^\s]+)"#).expect("Valid regex; qed;");

	//Skip the first argument (the binary name).
	let args_string: String = args.into_iter().skip(1).collect::<Vec<String>>().join(" ");
	let args_string = path_pos_regex.replace_all(&args_string, "").to_string();
	let args_string = image_regex.replace_all(&args_string, "").to_string();
	let args_string = verify_regex.replace_all(&args_string, "").to_string();
	let args_string = path_regex
		.replace_all(&args_string, "--manifest-path $1/Cargo.toml")
		.to_string();

	// Turn it back to a vec, filtering out commands and arguments
	// that should not be passed to the docker build command
	let os_args: Vec<String> = args_string
		.split_ascii_whitespace()
		.filter(|a| a != &"--verifiable" && a != &"verify" && a != &"build")
		.map(|s| s.to_string())
		.collect();

	Ok(os_args)
}

/// Build the smart contract located at the specified `path` in `build_release` mode.
///
/// If `build_mode` is `Verifiable`, this function will call a docker image running a verifiable
/// build using some CLI (by default, `cargo contract`).
///
/// # Arguments
/// * `path` - The optional path to the smart contract manifest, defaulting to the current directory
///   if not specified.
/// * `release` - Whether the smart contract should be built without any debugging functionality.
/// * `verbosity` - The build output verbosity.
/// * `metadata_spec` - Optionally specify the contract metadata format/version.
pub fn build_smart_contract(
	path: &Path,
	build_mode: BuildMode,
	verbosity: Verbosity,
	metadata_spec: Option<MetadataSpec>,
	image: Option<ImageVariant>,
) -> anyhow::Result<BuildResult> {
	let manifest_path = get_manifest_path(path)?;

	let target_dir = manifest_path
		.absolute_directory()
		.ok()
		.map(|project_dir| project_dir.join("target"));

	let metadata_spec = match metadata_spec {
		s @ Some(_) => s,
		None => resolve_metadata_spec(manifest_path.as_ref())?,
	};

	let mut args = ExecuteArgs {
		manifest_path,
		build_mode,
		verbosity,
		metadata_spec,
		target_dir,
		..Default::default()
	};

	if let Some(image) = image {
		args.image = image;
	}

	// Execute the build and log the output of the build
	match build_mode {
		// For verifiable contracts, execute calls docker_build (https://github.com/use-ink/cargo-contract/blob/master/crates/build/src/lib.rs#L595) which launches a blocking tokio runtime to handle the async operations (https://github.com/use-ink/cargo-contract/blob/master/crates/build/src/docker.rs#L135). The issue is that pop is itself a tokio runtime, launching another blocking one isn't allowed by tokio. So for verifiable contracts we need to first block the main pop tokio runtime before calling execute
		BuildMode::Verifiable =>
			tokio::task::block_in_place(|| execute::<PopComposeBuildArgs>(args)),
		_ => execute::<PopComposeBuildArgs>(args),
	}
}

/// Determine the metadata spec to use inferring from the
/// `[package.metadata.ink-lang]` `abi` setting in `Cargo.toml`.
fn resolve_metadata_spec(manifest_path: &Path) -> anyhow::Result<Option<MetadataSpec>> {
	let manifest_contents = fs::read_to_string(manifest_path)?;
	let manifest: Value = toml::from_str(&manifest_contents)?;

	let abi = manifest
		.get("package")
		.and_then(Value::as_table)
		.and_then(|pkg| pkg.get("metadata"))
		.and_then(Value::as_table)
		.and_then(|metadata| metadata.get("ink-lang"))
		.and_then(Value::as_table)
		.and_then(|ink_lang| ink_lang.get("abi"))
		.and_then(Value::as_str)
		.map(|abi| abi.to_lowercase());

	Ok(match abi.as_deref() {
		// Prefer Solidity metadata when the contract is configured for Solidity or dual ABI.
		Some("sol") | Some("all") => Some(MetadataSpec::Solidity),
		_ => None,
	})
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
	use std::fs;

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

	#[test]
	fn resolve_metadata_spec_infers_solidity_for_sol_abi() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let manifest_path = temp_dir.path().join("Cargo.toml");
		fs::write(
			&manifest_path,
			r#"[package]
name = "dummy"
version = "0.1.0"
[package.metadata.ink-lang]
abi = "sol"
"#,
		)?;

		let spec = resolve_metadata_spec(&manifest_path)?;
		assert_eq!(spec, Some(MetadataSpec::Solidity));
		Ok(())
	}

	#[test]
	fn resolve_metadata_spec_infers_solidity_for_all_abi() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let manifest_path = temp_dir.path().join("Cargo.toml");
		fs::write(
			&manifest_path,
			r#"[package]
name = "dummy"
version = "0.1.0"
[package.metadata.ink-lang]
abi = "all"
"#,
		)?;

		let spec = resolve_metadata_spec(&manifest_path)?;
		assert_eq!(spec, Some(MetadataSpec::Solidity));
		Ok(())
	}

	#[test]
	fn resolve_metadata_spec_handles_explicit_ink_abi() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let manifest_path = temp_dir.path().join("Cargo.toml");
		fs::write(
			&manifest_path,
			r#"[package]
name = "dummy"
version = "0.1.0"
[package.metadata.ink-lang]
abi = "ink"
"#,
		)?;

		let spec = resolve_metadata_spec(&manifest_path)?;
		assert!(spec.is_none());
		Ok(())
	}

	#[test]
	fn resolve_metadata_spec_defaults_for_ink() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let manifest_path = temp_dir.path().join("Cargo.toml");
		fs::write(
			&manifest_path,
			r#"[package]
name = "dummy"
version = "0.1.0"
"#,
		)?;

		let spec = resolve_metadata_spec(&manifest_path)?;
		assert!(spec.is_none());
		Ok(())
	}

	#[test]
	fn process_build_args_transforms_path_to_manifest_path() {
		let args =
			vec!["pop".to_string(), "build".to_string(), "--path".to_string(), ".".to_string()];

		let result = process_build_args(args).unwrap();
		assert_eq!(result, vec!["--manifest-path".to_string(), "./Cargo.toml".to_string()]);
	}

	#[test]
	fn process_build_args_removes_pop_specific_args() {
		let args = vec![
			"pop".to_string(),
			"build".to_string(),
			"--verifiable".to_string(),
			"--image".to_string(),
			"some-image".to_string(),
			"--path-pos".to_string(),
			"./path".to_string(),
			"--release".to_string(),
		];

		let result = process_build_args(args).unwrap();
		assert_eq!(result, vec!["--release".to_string()]);
	}

	#[test]
	fn process_build_args_removes_verify_specific_args() {
		let args = vec![
			"pop".to_string(),
			"verify".to_string(),
			"--contract-path".to_string(),
			"bundle.contract".to_string(),
			"--url".to_string(),
			"wss://example.com".to_string(),
			"--address".to_string(),
			"0x123".to_string(),
			"--release".to_string(),
		];

		let result = process_build_args(args).unwrap();
		assert_eq!(result, vec!["--release".to_string()]);
	}
}
