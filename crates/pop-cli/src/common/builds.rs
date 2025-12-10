// SPDX-License-Identifier: GPL-3.0

use std::{env::current_dir, path::PathBuf};
#[cfg(feature = "chain")]
use {
	crate::cli::traits::{Cli, Select},
	pop_chains::ChainSpecBuilder,
	pop_common::{
		Profile,
		manifest::{Manifest, get_workspace_project_names},
	},
	std::path::Path,
	strum::{EnumMessage, VariantArray},
};
#[cfg(feature = "contract")]
use {pop_contracts::ComposeBuildArgs, regex::Regex};

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

#[cfg(feature = "chain")]
/// Represent how the contained Path should be used.
pub(crate) enum ChainPath {
	/// The path's going to be used to search for something else inside it (eg, a runtime, node,...)
	Base(PathBuf),
	/// The path is the exact one that should be used
	Exact(PathBuf),
}

/// Creates a chain specification builder based on project structure.
///
/// # Arguments
/// * `path` - Path to the project. If `[ChainPath::Exact]` is used, it'll point to a runtime.
/// * `profile` - Build profile to use.
/// * `default_bootnode` - Whether to use default bootnode.
/// * `cli` - Command line interface implementation.
///
/// # Returns
/// The chain spec builder for the node or the runtime.
#[cfg(feature = "chain")]
pub fn create_chain_spec_builder(
	path: ChainPath,
	profile: &Profile,
	default_bootnode: bool,
	cli: &mut impl Cli,
) -> anyhow::Result<ChainSpecBuilder> {
	match path {
		ChainPath::Base(path) => {
			let default_node_path = path.join("node");
			if default_node_path.is_dir() {
				let node_path = default_node_path.canonicalize()?;
				cli.info(format!("Using node at {}", node_path.display()))?;
				Ok(ChainSpecBuilder::Node { node_path, default_bootnode, profile: *profile })
			} else {
				let runtime_path = find_runtime_dir(&path, cli)?;
				cli.info(format!("Using runtime at {}", runtime_path.display()))?;
				Ok(ChainSpecBuilder::Runtime { runtime_path, profile: *profile })
			}
		},
		ChainPath::Exact(runtime_path) => {
			let runtime_path = runtime_path.canonicalize()?;
			cli.info(format!("Using runtime at {}", runtime_path.display()))?;

			Ok(ChainSpecBuilder::Runtime {
				runtime_path: runtime_path.to_owned(),
				profile: *profile,
			})
		},
	}
}

/// Finds the runtime directory in a project, prompting user selection if multiple candidates exist.
///
/// # Arguments
/// * `project_path` - Path to the project.
/// * `cli` - Command line interface implementation.
///
/// # Returns
/// Path to the selected runtime directory.
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
	Ok(*prompt.interact()?)
}

#[cfg(feature = "contract")]
pub(crate) struct PopComposeBuildArgs;
#[cfg(feature = "contract")]
impl ComposeBuildArgs for PopComposeBuildArgs {
	fn compose_build_args() -> anyhow::Result<Vec<String>> {
		let mut args: Vec<String> = Vec::new();
		// match pop related args in pop build --verifiable or pop verify that shouldn't be passed
		// to Docker image. --path-pos and its following value should be ignored anyway
		let path_pos_regex = Regex::new(r#"(--path-pos)[ ]*[^ ]*[ ]*"#).expect("Valid regex; qed;");
		// If --image is passed in build command, remove it
		let image_regex = Regex::new(r#"(--image)[ ]*[^ ]*[ ]*"#).expect("Valid regex; qed;");
		// If verify, we ignore --contract-path and its value, --url and its value and --address and its value
		let verify_regex =
			Regex::new(r#"(--contract-path|--url|--address)[ ]*[^ ]*[ ]*"#).expect("Valid regex; qed;");

		// we join the args together, so we can remove `--image <arg>`. Skip the first argument (the
		// binary name)
		let args_string: String = std::env::args().skip(1).collect::<Vec<String>>().join(" ");
		let args_string = path_pos_regex.replace_all(&args_string, "").to_string();
		let args_string = image_regex.replace_all(&args_string, "").to_string();
		let args_string = verify_regex.replace_all(&args_string, "").to_string();

		// and then we turn it back to the vec, filtering out commands and arguments
		// that should not be passed to the docker build command
		let mut os_args: Vec<String> = args_string
			.split_ascii_whitespace()
			.filter(|a| a != &"--verifiable" && a != &"verify" && a != &"build")
			.map(|s| s.to_string())
			.collect();

		args.append(&mut os_args);

		Ok(args)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[cfg(feature = "chain")]
	use {crate::cli::MockCli, std::fs, tempfile::tempdir};

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

	#[test]
	#[cfg(feature = "chain")]
	fn create_chain_spec_builder_with_node_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;

		// Create node directory
		let node_dir = temp_dir.path().join("node");
		fs::create_dir(&node_dir)?;

		let mut cli = MockCli::new()
			.expect_info(format!("Using node at {}", node_dir.canonicalize()?.display()));

		let result = create_chain_spec_builder(
			ChainPath::Base(temp_dir.path().to_path_buf()),
			&Profile::Release,
			true,
			&mut cli,
		)?;

		// Verify it returns ChainSpecBuilder::Node variant
		match result {
			ChainSpecBuilder::Node { node_path, default_bootnode, profile } => {
				assert_eq!(node_path, node_dir.canonicalize()?);
				assert!(default_bootnode);
				assert_eq!(profile, Profile::Release);
			},
			_ => panic!("Expected ChainSpecBuilder::Node variant"),
		}

		cli.verify()
	}

	#[test]
	#[cfg(feature = "chain")]
	fn create_chain_spec_builder_with_runtime_works_using_base_path() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;

		// Create workspace structure
		let workspace_toml = temp_dir.path().join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["runtime"]

[workspace.package]
name = "test-workspace"
"#,
		)?;

		// Create runtime directory (no node directory)
		let runtime_dir = temp_dir.path().join("runtime");
		fs::create_dir(&runtime_dir)?;
		fs::write(
			runtime_dir.join("Cargo.toml"),
			r#"[package]
name = "runtime"
version = "0.1.0"
"#,
		)?;

		let mut cli = MockCli::new()
			.expect_info(format!("Using runtime at {}", runtime_dir.canonicalize()?.display()));

		let result = create_chain_spec_builder(
			ChainPath::Base(temp_dir.path().to_path_buf()),
			&Profile::Release,
			false,
			&mut cli,
		)?;

		// Verify it returns ChainSpecBuilder::Runtime variant
		match result {
			ChainSpecBuilder::Runtime { runtime_path, profile } => {
				assert_eq!(runtime_path, runtime_dir.canonicalize()?);
				assert_eq!(profile, Profile::Release);
			},
			_ => panic!("Expected ChainSpecBuilder::Runtime variant"),
		}

		cli.verify()
	}

	#[test]
	#[cfg(feature = "chain")]
	fn create_chain_spec_builder_with_runtime_works_using_exact_path() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;

		// Create workspace structure
		let workspace_toml = temp_dir.path().join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["runtime"]

[workspace.package]
name = "test-workspace"
"#,
		)?;

		// Create runtime directory (no node directory)
		let runtime_dir_not_called_runtime = temp_dir.path().join("something");
		fs::create_dir(&runtime_dir_not_called_runtime)?;
		fs::write(
			runtime_dir_not_called_runtime.join("Cargo.toml"),
			r#"[package]
name = "runtime"
version = "0.1.0"
"#,
		)?;

		let mut cli = MockCli::new().expect_info(format!(
			"Using runtime at {}",
			runtime_dir_not_called_runtime.canonicalize()?.display()
		));

		let result = create_chain_spec_builder(
			ChainPath::Exact(runtime_dir_not_called_runtime.clone()),
			&Profile::Release,
			false,
			&mut cli,
		)?;

		// Verify it returns ChainSpecBuilder::Runtime variant
		match result {
			ChainSpecBuilder::Runtime { runtime_path, profile } => {
				assert_eq!(runtime_path, runtime_dir_not_called_runtime.canonicalize()?);
				assert_eq!(profile, Profile::Release);
			},
			_ => panic!("Expected ChainSpecBuilder::Runtime variant"),
		}

		cli.verify()
	}

	#[test]
	#[cfg(feature = "chain")]
	fn create_chain_spec_builder_with_runtime_using_exact_path_fails_if_path_cannot_be_canonicalized()
	-> anyhow::Result<()> {
		let temp_dir = tempdir()?;

		// Create workspace structure
		let workspace_toml = temp_dir.path().join("Cargo.toml");
		fs::write(
			&workspace_toml,
			r#"[workspace]
members = ["runtime"]

[workspace.package]
name = "test-workspace"
"#,
		)?;

		// Create runtime directory (no node directory)
		let runtime_dir_not_called_runtime = temp_dir.path().join("something");
		let mut cli = MockCli::new().expect_info("nothing".to_string());

		let result = create_chain_spec_builder(
			ChainPath::Exact(runtime_dir_not_called_runtime),
			&Profile::Release,
			false,
			&mut cli,
		);

		match result {
			Err(err) if err.downcast_ref::<std::io::Error>().is_some() => {
				assert_eq!(
					err.downcast_ref::<std::io::Error>().unwrap().kind(),
					std::io::ErrorKind::NotFound
				);
				Ok(())
			},
			_ => panic!("The dir doesn't exist"),
		}
	}
}
