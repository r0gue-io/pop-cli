// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use duct::cmd;
use std::path::Path;

/// Build the parachain located in the specified `path`.
///
/// # Arguments
/// * `path` - The optional path to the parachain manifest, defaulting to the current directory if not specified.
/// * `package` - The optional package to be built.
/// * `release` - Whether the parachain should be built without any debugging functionality.
pub fn build_parachain(
	path: Option<&Path>,
	package: Option<String>,
	release: bool,
) -> anyhow::Result<()> {
	let mut args = vec!["build"];
	if let Some(package) = package.as_deref() {
		args.push("--package");
		args.push(package)
	}
	if release {
		args.push("--release");
	}
	cmd("cargo", args).dir(path.unwrap_or_else(|| Path::new("./"))).run()?;
	Ok(())
}

/// Determines whether the manifest at the supplied path is a supported parachain project.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not specified.
pub fn is_supported(path: Option<&Path>) -> Result<bool, Error> {
	let manifest = pop_common::manifest::from_path(path)?;
	// Simply check for a parachain dependency
	const DEPENDENCIES: [&str; 4] =
		["cumulus-client-collator", "cumulus-primitives-core", "parachains-common", "polkadot-sdk"];
	Ok(DEPENDENCIES.into_iter().any(|d| {
		manifest.dependencies.contains_key(d)
			|| manifest.workspace.as_ref().map_or(false, |w| w.dependencies.contains_key(d))
	}))
}

#[cfg(test)]
mod tests {
	use super::*;
	use duct::cmd;
	use pop_common::manifest::{self, Dependency};
	use std::fs::write;

	#[test]
	fn is_supported_works() -> anyhow::Result<()> {
		let temp_dir = tempfile::tempdir()?;
		let path = temp_dir.path();

		// Standard rust project
		let name = "hello_world";
		cmd("cargo", ["new", name]).dir(&path).run()?;
		assert!(!is_supported(Some(&path.join(name)))?);

		// Parachain
		let mut manifest = manifest::from_path(Some(&path.join(name)))?;
		manifest
			.dependencies
			.insert("cumulus-client-collator".into(), Dependency::Simple("^0.14.0".into()));
		let manifest = toml_edit::ser::to_string_pretty(&manifest)?;
		write(path.join(name).join("Cargo.toml"), manifest)?;
		assert!(is_supported(Some(&path.join(name)))?);
		Ok(())
	}
}
