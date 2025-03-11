// SPDX-License-Identifier: GPL-3.0
use crate::cli::traits::{Cli, Select};
use pop_common::Profile;
use pop_parachains::{binary_path, build_parachain};
use std::{env::current_dir, path::PathBuf};
use strum::{EnumMessage, VariantArray};

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
/// * `mode`: The profile to use for building.
/// * `features`: The features that node is built with.
pub fn ensure_node_binary_exists(
	cli: &mut impl Cli,
	mode: &Profile,
	features: Vec<&str>,
) -> anyhow::Result<PathBuf> {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	match binary_path(&mode.target_directory(&cwd), &cwd.join("node")) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("Node was not found. The project will be built locally.".to_string())?;
			cli.warning("NOTE: this may take some time...")?;
			build_parachain(&cwd, None, mode, None, features).map_err(|e| e.into())
		},
	}
}

/// Guide the user to select a build profile.
///
/// # Arguments
/// * `cli`: Command line interface.
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
