// SPDX-License-Identifier: GPL-3.0

use std::path::PathBuf;

#[cfg(feature = "parachain")]
use {
	crate::cli::traits::{Cli, Select},
	pop_common::Profile,
	pop_parachains::{binary_path, build_parachain},
	std::path::Path,
	strum::{EnumMessage, VariantArray},
};

/// This method is used to get the proper project path format (with or without cli flag)
pub fn get_project_path(path_flag: Option<PathBuf>, path_pos: Option<PathBuf>) -> Option<PathBuf> {
	let project_path = if let Some(ref path) = path_pos {
		Some(path) // Use positional path if present
	} else {
		path_flag.as_ref() // Otherwise, use the named path
	};
	project_path.cloned()
}

/// Locate node binary, if it doesn't exist trigger build.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `project_path`: The project path.
/// * `mode`: The profile to use for building.
/// * `features`: The features that node is built with.
#[cfg(feature = "parachain")]
pub fn ensure_node_binary_exists(
	cli: &mut impl Cli,
	project_path: &Path,
	mode: &Profile,
	features: Vec<&str>,
) -> anyhow::Result<PathBuf> {
	match binary_path(&mode.target_directory(project_path), &project_path.join("node")) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("Node was not found. The project will be built locally.".to_string())?;
			cli.warning("NOTE: this may take some time...")?;
			build_parachain(project_path, None, mode, None, features).map_err(|e| e.into())
		},
	}
}

/// Guide the user to select a build profile.
///
/// # Arguments
/// * `cli`: Command line interface.
#[cfg(feature = "parachain")]
pub fn guide_user_to_select_profile(cli: &mut impl Cli) -> anyhow::Result<Profile> {
	let default = Profile::Release;
	// Prompt for build profile.
	let mut prompt = cli
		.select("Choose the build profile of the binary that should be used: ".to_string())
		.initial_value(&default);
	for profile in Profile::VARIANTS {
		prompt = prompt.item(
			profile,
			profile.get_message().unwrap_or(profile.as_ref()),
			profile.get_detailed_message().unwrap_or_default(),
		);
	}
	Ok(prompt.interact()?.clone())
}

#[cfg(test)]
#[cfg(feature = "parachain")]
mod tests {
	use std::fs::{self, File};

	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	use tempfile::tempdir;

	#[test]
	#[cfg(feature = "parachain")]
	fn guide_user_to_select_profile_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new().expect_select(
			"Choose the build profile of the binary that should be used: ".to_string(),
			Some(true),
			true,
			Some(Profile::get_variants()),
			0,
			None,
		);
		guide_user_to_select_profile(&mut cli)?;
		cli.verify()
	}

	#[test]
	#[cfg(feature = "parachain")]
	fn ensure_node_binary_exists_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let name = "node";
		let temp_dir = tempdir()?;
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		let target_path = Profile::Release.target_directory(temp_dir.path());

		fs::create_dir(&temp_dir.path().join("target"))?;
		fs::create_dir(&target_path)?;
		File::create(target_path.join("node"))?;

		let binary_path =
			ensure_node_binary_exists(&mut cli, temp_dir.path(), &Profile::Release, vec![])?;
		assert_eq!(binary_path, target_path.join("node"));
		cli.verify()
	}
}
