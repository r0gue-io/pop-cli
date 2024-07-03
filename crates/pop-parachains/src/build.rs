// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use duct::cmd;
use std::path::Path;

/// Build the parachain located in the specified `path`.
///
/// # Arguments
/// * `path` - The optional path to the parachain manifest, defaulting to the current directory if not specified.
/// * `release` - Whether the parachain should be built without any debugging functionality.
pub fn build_parachain(path: Option<&Path>, release: bool) -> anyhow::Result<()> {
	let mut args = vec!["build"];
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
