// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use anyhow;
pub use cargo_toml::{Dependency, LtoSetting, Manifest, Profile, Profiles};
use std::{
	fs::{read_to_string, write},
	path::{Path, PathBuf},
};
use toml_edit::{value, Array, DocumentMut, Item, Value};

/// Parses the contents of a `Cargo.toml` manifest.
///
/// # Arguments
/// * `path` - The optional path to the manifest, defaulting to the current directory if not
///   specified.
pub fn from_path(path: Option<&Path>) -> Result<Manifest, Error> {
	// Resolve manifest path
	let path = match path {
		Some(path) => match path.ends_with("Cargo.toml") {
			true => path.to_path_buf(),
			false => path.join("Cargo.toml"),
		},
		None => PathBuf::from("./Cargo.toml"),
	};
	if !path.exists() {
		return Err(Error::ManifestPath(path.display().to_string()));
	}
	Ok(Manifest::from_path(path.canonicalize()?)?)
}

/// This function is used to determine if a Path is contained inside a workspace, and returns a
/// PathBuf to the workspace Cargo.toml if found.
///
/// # Arguments
/// * `target_dir` - A directory that may be contained inside a workspace
pub fn find_workspace_toml(target_dir: &Path) -> Option<PathBuf> {
	let mut dir = target_dir;
	while let Some(parent) = dir.parent() {
		// This condition is necessary to avoid that calling the function from a workspace using a
		// path which isn't contained in a workspace returns `Some(Cargo.toml)` refering the
		// workspace from where the function has been called instead of the expected `None`.
		if parent.to_str() == Some("") {
			return None;
		}
		let cargo_toml = parent.join("Cargo.toml");
		if cargo_toml.exists() {
			if let Ok(contents) = read_to_string(&cargo_toml) {
				if contents.contains("[workspace]") {
					return Some(cargo_toml);
				}
			}
		}
		dir = parent;
	}
	None
}

/// This function is used to add a crate to a workspace.
/// # Arguments
///
/// * `workspace_toml` - The path to the workspace `Cargo.toml`
/// * `crate_path`: The path to the crate that should be added to the workspace
pub fn add_crate_to_workspace(workspace_toml: &Path, crate_path: &Path) -> anyhow::Result<()> {
	let toml_contents = read_to_string(workspace_toml)?;
	let mut doc = toml_contents.parse::<DocumentMut>()?;

	// Find the workspace dir
	let workspace_dir = workspace_toml.parent().expect("A file always lives inside a dir; qed");
	// Find the relative path to the crate from the workspace root
	let crate_relative_path = crate_path.strip_prefix(workspace_dir)?;

	if let Some(Item::Table(workspace_table)) = doc.get_mut("workspace") {
		if let Some(Item::Value(members_array)) = workspace_table.get_mut("members") {
			if let Value::Array(array) = members_array {
				let crate_relative_path =
					crate_relative_path.to_str().expect("target's always a valid string; qed");
				let already_in_array = array
					.iter()
					.any(|member| matches!(member.as_str(), Some(s) if s == crate_relative_path));
				if !already_in_array {
					array.push(crate_relative_path);
				}
			} else {
				return Err(anyhow::anyhow!("Corrupted workspace"));
			}
		} else {
			let mut toml_array = Array::new();
			toml_array
				.push(crate_relative_path.to_str().expect("target's always a valid string; qed"));
			workspace_table["members"] = value(toml_array);
		}
	} else {
		return Err(anyhow::anyhow!("Corrupted workspace"));
	}

	write(workspace_toml, doc.to_string())?;
	Ok(())
}

/// Adds a "production" profile to the Cargo.toml manifest if it doesn't already exist.
///
/// # Arguments
/// * `project` - The path to the root of the Cargo project containing the Cargo.toml.
pub fn add_production_profile(project: &Path) -> anyhow::Result<()> {
	let root_toml_path = project.join("Cargo.toml");
	let mut manifest = Manifest::from_path(&root_toml_path)?;
	// Check if the `production` profile already exists.
	if manifest.profile.custom.contains_key("production") {
		return Ok(());
	}
	// Create the production profile with required fields.
	let production_profile = Profile {
		opt_level: None,
		debug: None,
		split_debuginfo: None,
		rpath: None,
		lto: Some(LtoSetting::Fat),
		debug_assertions: None,
		codegen_units: Some(1),
		panic: None,
		incremental: None,
		overflow_checks: None,
		strip: None,
		package: std::collections::BTreeMap::new(),
		build_override: None,
		inherits: Some("release".to_string()),
	};
	// Insert the new profile into the custom profiles
	manifest.profile.custom.insert("production".to_string(), production_profile);

	// Serialize the updated manifest and write it back to the file
	let toml_string = toml::to_string(&manifest)?;
	write(&root_toml_path, toml_string)?;

	Ok(())
}

/// Add a new feature to the Cargo.toml manifest if it doesn't already exist.
///
/// # Arguments
/// * `project` - The path to the project directory.
/// * `(key, items)` - The feature key and its associated items.
pub fn add_feature(project: &Path, (key, items): (String, Vec<String>)) -> anyhow::Result<()> {
	let root_toml_path = project.join("Cargo.toml");
	let mut manifest = Manifest::from_path(&root_toml_path)?;
	// Check if the feature already exists.
	if manifest.features.contains_key(&key) {
		return Ok(());
	}
	manifest.features.insert(key, items);

	// Serialize the updated manifest and write it back to the file
	let toml_string = toml::to_string(&manifest)?;
	write(&root_toml_path, toml_string)?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs::{write, File};
	use tempfile::TempDir;

	struct TestBuilder {
		main_tempdir: TempDir,
		workspace: Option<TempDir>,
		inside_workspace_dir: Option<TempDir>,
		workspace_cargo_toml: Option<PathBuf>,
		outside_workspace_dir: Option<TempDir>,
	}

	impl Default for TestBuilder {
		fn default() -> Self {
			Self {
				main_tempdir: TempDir::new().expect("Failed to create tempdir"),
				workspace: None,
				inside_workspace_dir: None,
				workspace_cargo_toml: None,
				outside_workspace_dir: None,
			}
		}
	}

	impl TestBuilder {
		fn add_workspace(self) -> Self {
			Self { workspace: TempDir::new_in(self.main_tempdir.as_ref()).ok(), ..self }
		}

		fn add_inside_workspace_dir(self) -> Self {
			Self {
				inside_workspace_dir: TempDir::new_in(self.workspace.as_ref().expect(
					"add_inside_workspace_dir is only callable if workspace has been created",
				))
				.ok(),
				..self
			}
		}

		fn add_workspace_cargo_toml(self, cargo_toml_content: &str) -> Self {
			let workspace_cargo_toml = self
				.workspace
				.as_ref()
				.expect("add_workspace_cargo_toml is only callable if workspace has been created")
				.path()
				.join("Cargo.toml");
			File::create(&workspace_cargo_toml).expect("Failed to create Cargo.toml");
			write(&workspace_cargo_toml, cargo_toml_content).expect("Failed to write Cargo.toml");
			Self { workspace_cargo_toml: Some(workspace_cargo_toml.to_path_buf()), ..self }
		}

		fn add_outside_workspace_dir(self) -> Self {
			Self { outside_workspace_dir: TempDir::new_in(self.main_tempdir.as_ref()).ok(), ..self }
		}
	}

	#[test]
	fn from_path_works() -> anyhow::Result<()> {
		// Workspace manifest from directory
		from_path(Some(Path::new("../../")))?;
		// Workspace manifest from path
		from_path(Some(Path::new("../../Cargo.toml")))?;
		// Package manifest from directory
		from_path(Some(Path::new(".")))?;
		// Package manifest from path
		from_path(Some(Path::new("./Cargo.toml")))?;
		// None
		from_path(None)?;
		Ok(())
	}

	#[test]
	fn from_path_ensures_manifest_exists() -> Result<(), Error> {
		assert!(matches!(
			from_path(Some(Path::new("./none.toml"))),
			Err(super::Error::ManifestPath(..))
		));
		Ok(())
	}

	#[test]
	fn find_workspace_toml_works_well() {
		let test_builder = TestBuilder::default()
			.add_workspace()
			.add_inside_workspace_dir()
			.add_workspace_cargo_toml(
				r#"[workspace]
                resolver = "2"
                members = ["member1"]
                "#,
			)
			.add_outside_workspace_dir();
		assert!(find_workspace_toml(
			test_builder
				.inside_workspace_dir
				.as_ref()
				.expect("Inside workspace dir should exist")
				.path()
		)
		.is_some());
		assert_eq!(
			find_workspace_toml(
				test_builder
					.inside_workspace_dir
					.as_ref()
					.expect("Inside workspace dir should exist")
					.path()
			)
			.expect("The Cargo.toml should exist at this point"),
			test_builder.workspace_cargo_toml.expect("Cargo.toml should exist")
		);
		assert!(find_workspace_toml(
			test_builder
				.outside_workspace_dir
				.as_ref()
				.expect("Outside workspace dir should exist")
				.path()
		)
		.is_none());
		// Calling the function from a relative path which parent is "" returns None
		assert!(find_workspace_toml(&PathBuf::from("..")).is_none());
	}

	#[test]
	fn add_crate_to_workspace_works_well_if_members_exists() {
		let test_builder = TestBuilder::default()
			.add_workspace()
			.add_workspace_cargo_toml(
				r#"[workspace]
                resolver = "2"
                members = ["member1"]
                "#,
			)
			.add_inside_workspace_dir();
		let add_crate = add_crate_to_workspace(
			test_builder.workspace_cargo_toml.as_ref().expect("Workspace should exist"),
			test_builder
				.inside_workspace_dir
				.as_ref()
				.expect("Inside workspace dir should exist")
				.path(),
		);
		assert!(add_crate.is_ok());
		let content = read_to_string(
			test_builder.workspace_cargo_toml.as_ref().expect("Workspace should exist"),
		)
		.expect("Cargo.toml should be readable");
		let doc = content.parse::<DocumentMut>().expect("This should work");
		if let Some(Item::Table(workspace_table)) = doc.get("workspace") {
			if let Some(Item::Value(Value::Array(array))) = workspace_table.get("members") {
				assert!(array.iter().any(|item| {
					if let Value::String(item) = item {
						// item is only the relative path from the Cargo.toml manifest, while
						// test_buildder.insider_workspace_dir is the absolute path, so we can only
						// test with contains
						test_builder
							.inside_workspace_dir
							.as_ref()
							.expect("Inside workspace should exist")
							.path()
							.to_str()
							.expect("Dir should be mapped to a str")
							.contains(item.value())
					} else {
						false
					}
				}));
			} else {
				panic!("This shouldn't be reached");
			}
		} else {
			panic!("This shouldn't be reached");
		}

		// Calling with a crate that's already in the workspace doesn't include it twice
		let add_crate = add_crate_to_workspace(
			test_builder.workspace_cargo_toml.as_ref().expect("Workspace should exist"),
			test_builder
				.inside_workspace_dir
				.as_ref()
				.expect("Inside workspace dir should exist")
				.path(),
		);
		assert!(add_crate.is_ok());
		let doc = content.parse::<DocumentMut>().expect("This should work");
		if let Some(Item::Table(workspace_table)) = doc.get("workspace") {
			if let Some(Item::Value(Value::Array(array))) = workspace_table.get("members") {
				assert_eq!(
					array
						.iter()
						.filter(|item| {
							if let Value::String(item) = item {
								test_builder
									.inside_workspace_dir
									.as_ref()
									.expect("Inside workspace should exist")
									.path()
									.to_str()
									.expect("Dir should be mapped to a str")
									.contains(item.value())
							} else {
								false
							}
						})
						.count(),
					1
				);
			} else {
				panic!("This shouldn't be reached");
			}
		} else {
			panic!("This shouldn't be reached");
		}
	}

	#[test]
	fn add_crate_to_workspace_works_well_if_members_doesnt_exist() {
		let test_builder = TestBuilder::default()
			.add_workspace()
			.add_workspace_cargo_toml(
				r#"[workspace]
                resolver = "2"
                "#,
			)
			.add_inside_workspace_dir();
		let add_crate = add_crate_to_workspace(
			test_builder.workspace_cargo_toml.as_ref().expect("Workspace should exist"),
			test_builder
				.inside_workspace_dir
				.as_ref()
				.expect("Inside workspace dir should exist")
				.path(),
		);
		assert!(add_crate.is_ok());
		let content = read_to_string(
			test_builder.workspace_cargo_toml.as_ref().expect("Workspace should exist"),
		)
		.expect("Cargo.toml should be readable");
		let doc = content.parse::<DocumentMut>().expect("This should work");
		if let Some(Item::Table(workspace_table)) = doc.get("workspace") {
			if let Some(Item::Value(Value::Array(array))) = workspace_table.get("members") {
				assert!(array.iter().any(|item| {
					if let Value::String(item) = item {
						test_builder
							.inside_workspace_dir
							.as_ref()
							.expect("Inside workspace should exist")
							.path()
							.to_str()
							.expect("Dir should be mapped to a str")
							.contains(item.value())
					} else {
						false
					}
				}));
			} else {
				panic!("This shouldn't be reached");
			}
		} else {
			panic!("This shouldn't be reached");
		}
	}

	#[test]
	fn add_crate_to_workspace_fails_if_crate_path_not_inside_workspace() {
		let test_builder = TestBuilder::default()
			.add_workspace()
			.add_workspace_cargo_toml(
				r#"[workspace]
                resolver = "2"
                members = ["member1"]
                "#,
			)
			.add_outside_workspace_dir();
		let add_crate = add_crate_to_workspace(
			test_builder.workspace_cargo_toml.as_ref().expect("Workspace should exist"),
			test_builder
				.outside_workspace_dir
				.expect("Inside workspace dir should exist")
				.path(),
		);
		assert!(add_crate.is_err());
	}

	#[test]
	fn add_crate_to_workspace_fails_if_members_not_an_array() {
		let test_builder = TestBuilder::default()
			.add_workspace()
			.add_workspace_cargo_toml(
				r#"[workspace]
                resolver = "2"
                members = "member1"
                "#,
			)
			.add_inside_workspace_dir();
		let add_crate = add_crate_to_workspace(
			test_builder.workspace_cargo_toml.as_ref().expect("Workspace should exist"),
			test_builder
				.inside_workspace_dir
				.expect("Inside workspace dir should exist")
				.path(),
		);
		assert!(add_crate.is_err());
	}

	#[test]
	fn add_crate_to_workspace_fails_if_workspace_isnt_workspace() {
		let test_builder = TestBuilder::default()
			.add_workspace()
			.add_workspace_cargo_toml(r#""#)
			.add_inside_workspace_dir();
		let add_crate = add_crate_to_workspace(
			test_builder.workspace_cargo_toml.as_ref().expect("Workspace should exist"),
			test_builder
				.inside_workspace_dir
				.expect("Inside workspace dir should exist")
				.path(),
		);
		assert!(add_crate.is_err());
	}

	#[test]
	fn add_production_profile_works() {
		let test_builder = TestBuilder::default().add_workspace().add_workspace_cargo_toml(
			r#"[profile.release]
            opt-level = 3
            "#,
		);

		let binding = test_builder.workspace.expect("Workspace should exist");
		let project_path = binding.path();
		let cargo_toml_path = project_path.join("Cargo.toml");

		// Call the function to add the production profile
		let result = add_production_profile(project_path);
		assert!(result.is_ok());

		// Verify the production profile is added
		let manifest =
			Manifest::from_path(&cargo_toml_path).expect("Should parse updated Cargo.toml");
		let production_profile = manifest
			.profile
			.custom
			.get("production")
			.expect("Production profile should exist");
		assert_eq!(production_profile.codegen_units, Some(1));
		assert_eq!(production_profile.inherits.as_deref(), Some("release"));
		assert_eq!(production_profile.lto, Some(LtoSetting::Fat));

		// Test idempotency: Running the function again should not modify the manifest
		let initial_toml_content =
			read_to_string(&cargo_toml_path).expect("Cargo.toml should be readable");
		let second_result = add_production_profile(project_path);
		assert!(second_result.is_ok());
		let final_toml_content =
			read_to_string(&cargo_toml_path).expect("Cargo.toml should be readable");
		assert_eq!(initial_toml_content, final_toml_content);
	}

	#[test]
	fn add_feature_works() {
		let test_builder = TestBuilder::default().add_workspace().add_workspace_cargo_toml(
			r#"[profile.release]
            opt-level = 3
            "#,
		);

		let expected_feature_key = "runtime-benchmarks";
		let expected_feature_items =
			vec!["feature-a".to_string(), "feature-b".to_string(), "feature-c".to_string()];
		let binding = test_builder.workspace.expect("Workspace should exist");
		let project_path = binding.path();
		let cargo_toml_path = project_path.join("Cargo.toml");

		// Call the function to add the production profile
		let result = add_feature(
			project_path,
			(expected_feature_key.to_string(), expected_feature_items.clone()),
		);
		assert!(result.is_ok());

		// Verify the feature is added
		let manifest =
			Manifest::from_path(&cargo_toml_path).expect("Should parse updated Cargo.toml");
		let feature_items = manifest
			.features
			.get(expected_feature_key)
			.expect("Production profile should exist");
		assert_eq!(feature_items, &expected_feature_items);

		// Test idempotency: Running the function again should not modify the manifest
		let initial_toml_content =
			read_to_string(&cargo_toml_path).expect("Cargo.toml should be readable");
		let second_result = add_feature(
			project_path,
			(expected_feature_key.to_string(), expected_feature_items.clone()),
		);
		assert!(second_result.is_ok());
		let final_toml_content =
			read_to_string(&cargo_toml_path).expect("Cargo.toml should be readable");
		assert_eq!(initial_toml_content, final_toml_content);
	}
}
