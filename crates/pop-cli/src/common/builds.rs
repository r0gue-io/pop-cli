// SPDX-License-Identifier: GPL-3.0

use std::{env::current_dir, path::PathBuf};

#[cfg(feature = "chain")]
use {
	crate::cli::traits::{Cli, Select},
	pop_chains::{binary_path, build_chain},
	pop_common::{
		Profile,
		manifest::{Manifest, get_workspace_project_names},
	},
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

/// This method is used to get the proper project path format (with or without cli flag). Defaults
/// to the current directory.
pub fn ensure_project_path(path_flag: Option<PathBuf>, path_pos: Option<PathBuf>) -> PathBuf {
	get_project_path(path_flag, path_pos)
		.unwrap_or_else(|| current_dir().expect("Unable to get current directory"))
}

/// Locate node binary, if it doesn't exist trigger build.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `project_path`: The project path.
/// * `mode`: The profile to use for building.
/// * `features`: The features that node is built with.
#[cfg(feature = "chain")]
pub fn ensure_node_binary_exists(
	cli: &mut impl Cli,
	project_path: &Path,
	mode: &Profile,
	features: &[String],
) -> anyhow::Result<PathBuf> {
	match binary_path(&mode.target_directory(project_path), &project_path.join("node")) {
		Ok(binary_path) => Ok(binary_path),
		_ => {
			cli.info("Node was not found. The project will be built locally.".to_string())?;
			cli.warning("NOTE: this may take some time...")?;
			build_chain(project_path, None, mode, None, features).map_err(|e| e.into())
		},
	}
}

#[cfg(feature = "chain")]
pub fn find_runtime_dir(project_path: &Path, cli: &mut impl Cli) -> anyhow::Result<PathBuf> {
	let default_runtime_path = project_path.join("runtime");
	let runtime_path =
		if default_runtime_path.is_dir() && Manifest::from_path(&default_runtime_path).is_ok() {
			default_runtime_path
		} else {
			let projects = get_workspace_project_names(project_path)?
				.into_iter()
				.filter(|(name, path)| {
					name.contains("runtime") || path.to_string_lossy().contains("runtime")
				})
				.collect::<Vec<_>>();
			if projects.is_empty() {
				return Err(anyhow::anyhow!("No runtime project found in the workspace"));
			} else if projects.len() == 1 {
				// If there is only one runtime project, use it.
				projects[0].1.clone()
			} else {
				// Ask the user where is the runtime if needed
				let mut prompt = cli.select("Choose the runtime project:".to_string());
				for (name, path) in &projects {
					prompt = prompt.item(name.as_str(), name.clone(), path.to_string_lossy());
				}
				let selected = prompt.interact()?;
				projects
					.iter()
					.find(|(name, _)| name == selected)
					.expect("Selected path must exist")
					.to_owned()
					.1
			}
		};
	Ok(runtime_path.canonicalize()?)
}

/// Guide the user to select a build profile.
///
/// # Arguments
/// * `cli`: Command line interface.
#[cfg(feature = "chain")]
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
#[cfg(feature = "chain")]
mod tests {
	use std::fs::{self, File};

	use super::*;
	use crate::cli::MockCli;
	use duct::cmd;
	use tempfile::tempdir;

	#[test]
	#[cfg(feature = "chain")]
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
	#[cfg(feature = "chain")]
	fn ensure_node_binary_exists_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let name = "node";
		let temp_dir = tempdir()?;
		cmd("cargo", ["new", name, "--bin"]).dir(temp_dir.path()).run()?;
		let target_path = Profile::Release.target_directory(temp_dir.path());

		fs::create_dir(temp_dir.path().join("target"))?;
		fs::create_dir(&target_path)?;
		File::create(target_path.join("node"))?;

		let binary_path =
			ensure_node_binary_exists(&mut cli, temp_dir.path(), &Profile::Release, &[])?;
		assert_eq!(binary_path, target_path.join("node"));
		cli.verify()
	}

	#[test]
	fn get_project_path_works() {
		// Test with positional path
		let pos_path = Some(PathBuf::from("/path/to/project"));
		let flag_path = Some(PathBuf::from("/another/path"));
		assert_eq!(get_project_path(flag_path.clone(), pos_path.clone()), pos_path);

		// Test with flag path only
		assert_eq!(get_project_path(flag_path.clone(), None), flag_path);

		// Test with neither
		assert_eq!(get_project_path(None, None), None);
	}

	#[test]
	fn ensure_project_path_works() {
		// Test with positional path
		let pos_path = Some(PathBuf::from("."));
		assert_eq!(ensure_project_path(None, pos_path.clone()), PathBuf::from("."));

		// Test with flag path
		let flag_path = Some(PathBuf::from("."));
		assert_eq!(ensure_project_path(flag_path.clone(), None), PathBuf::from("."));

		// Test with neither - should return current directory
		let result = ensure_project_path(None, None);
		assert_eq!(result, current_dir().expect("Unable to get current directory"));
	}

	#[test]
	#[cfg(feature = "chain")]
	fn find_runtime_dir_with_default_path_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let temp_dir = tempdir()?;

		let workspace_toml = temp_dir.path().join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["runtime"]

[workspace.package]
name = "test-workspace"
"#,
		)?;

		// Create default runtime directory
		let runtime_dir = temp_dir.path().join("runtime");
		// Along with its Cargo.toml file
		fs::create_dir(&runtime_dir)?;
		fs::write(
			runtime_dir.join("Cargo.toml"),
			r#"[package]
name = "runtime"
version = "0.1.0"

[dependencies]
"#,
		)?;

		let result = find_runtime_dir(temp_dir.path(), &mut cli)?;
		assert_eq!(result, runtime_dir.canonicalize()?);
		cli.verify()
	}

	#[test]
	#[cfg(feature = "chain")]
	fn find_runtime_dir_with_single_workspace_runtime_works() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let temp_dir = tempdir()?;

		// Create workspace structure
		let workspace_toml = temp_dir.path().join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["my-runtime"]

[workspace.package]
name = "test-workspace"
"#,
		)?;

		// Create runtime project
		let runtime_path = temp_dir.path().join("my-runtime");
		fs::create_dir(&runtime_path)?;
		fs::write(
			runtime_path.join("Cargo.toml"),
			r#"[package]
name = "my-runtime"
version = "0.1.0"
"#,
		)?;

		let result = find_runtime_dir(temp_dir.path(), &mut cli)?;
		assert_eq!(result, runtime_path.canonicalize()?);
		cli.verify()
	}

	#[test]
	#[cfg(feature = "chain")]
	fn find_runtime_dir_with_multiple_runtimes_prompts_user() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;

		// Create workspace structure
		let workspace_toml = temp_dir.path().join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["runtime1", "runtime2"]

[workspace.package]
name = "test-workspace"
"#,
		)?;

		// Create first runtime project
		let runtime1_path = temp_dir.path().join("runtime1");
		fs::create_dir(&runtime1_path)?;
		fs::write(
			runtime1_path.join("Cargo.toml"),
			r#"[package]
name = "runtime1"
version = "0.1.0"
"#,
		)?;

		// Create second runtime project
		let runtime2_path = temp_dir.path().join("runtime2");
		fs::create_dir(&runtime2_path)?;
		fs::write(
			runtime2_path.join("Cargo.toml"),
			r#"[package]
name = "runtime2"
version = "0.1.0"
"#,
		)?;

		let mut cli = MockCli::new().expect_select(
			"Choose the runtime project:".to_string(),
			Some(true),
			true,
			None,
			0,
			None,
		);

		let result = find_runtime_dir(temp_dir.path(), &mut cli)?;
		// Should return one of the runtimes (the selected one)
		assert!(result == runtime1_path.canonicalize()? || result == runtime2_path.canonicalize()?);
		cli.verify()
	}

	#[test]
	#[cfg(feature = "chain")]
	fn find_runtime_dir_fails_when_no_runtime_found() -> anyhow::Result<()> {
		let mut cli = MockCli::new();
		let temp_dir = tempdir()?;

		// Create workspace structure without runtime
		let workspace_toml = temp_dir.path().join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["some-other-crate"]

[workspace.package]
name = "test-workspace"
"#,
		)?;

		// Create non-runtime project
		let other_path = temp_dir.path().join("some-other-crate");
		fs::create_dir(&other_path)?;
		fs::write(
			other_path.join("Cargo.toml"),
			r#"[package]
name = "some-other-crate"
version = "0.1.0"
"#,
		)?;

		let result = find_runtime_dir(temp_dir.path(), &mut cli);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("No runtime project found"));
		Ok(())
	}
}
