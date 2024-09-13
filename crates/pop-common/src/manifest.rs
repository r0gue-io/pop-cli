// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use anyhow;
pub use cargo_toml::{Dependency, Manifest};
use std::{
	collections::HashSet,
	fs::{read_to_string, write},
	path::{Path, PathBuf},
};
use toml_edit::{value, DocumentMut, Item, Value};

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
/// PathBuf to the workspace Cargo.toml if found
///
/// # Arguments
/// * `target_dir` - A directory that may be contained inside a workspace
pub fn find_workspace_toml(target_dir: &Path) -> Option<PathBuf> {
	let mut dir = target_dir;
	while let Some(parent) = dir.parent() {
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
/// * `workspace_toml` - The path to the workspace `Cargo.toml`
/// * `crate_path`: The path to the crate that should be added to the workspace
pub fn add_crate_to_workspace(workspace_toml: &Path, crate_path: &Path) -> anyhow::Result<()> {
	let toml_contents = read_to_string(workspace_toml)?;
	let mut doc = toml_contents.parse::<DocumentMut>()?;

	// Find the workspace dir
	let workspace_dir = workspace_toml.parent().expect("A file always lives inside a dir; qed");
	// Find the relative path to the crate from the workspace root
	let crate_relative_path = crate_path.strip_prefix(workspace_dir)?;

	if let Some(workspace) = doc.get_mut("workspace") {
		if let Item::Table(workspace_table) = workspace {
			if let Some(members) = workspace_table.get_mut("members") {
				if let Item::Value(members_array) = members {
					if let Value::Array(array) = members_array {
						array.push(
							crate_relative_path
								.to_str()
								.expect("target's always a valid string; qed"),
						);
					} else {
						return Err(anyhow::anyhow!("Corrupted workspace"));
					}
				}
			} else {
				workspace_table["members"] = value(
					crate_relative_path.to_str().expect("target's always a valid string; qed"),
				);
			}
		}
	} else {
		return Err(anyhow::anyhow!("Corrupted workspace"));
	}

	write(workspace_toml, doc.to_string())?;
	Ok(())
}

/// Collects the dependencies of the given `Cargo.toml` manifests in a HashMap.
///
/// # Arguments
/// * `manifests`: Paths of the manifests to collect dependencies from.
pub fn collect_manifest_dependencies(manifests: Vec<&Path>) -> anyhow::Result<HashSet<String>> {
	let mut dependencies = HashSet::new();
	for m in manifests {
		let cargo = &std::fs::read_to_string(m)?.parse::<DocumentMut>()?;
		for d in cargo["dependencies"].as_table().unwrap().into_iter() {
			dependencies.insert(d.0.into());
		}
		for d in cargo["build-dependencies"].as_table().unwrap().into_iter() {
			dependencies.insert(d.0.into());
		}
	}
	Ok(dependencies)
}

/// Extends `target` cargo manifest with `dependencies` using versions from `source`.
///
/// It is useful when the list of dependencies is compiled from some external crates,
/// but there is interest on using a separate manifest as the source of truth for their versions.
///
/// # Arguments
/// * `target`: Manifest to be modified.
/// * `source`: Manifest used as reference for the dependency versions.
/// * `template`: The name of the template for the target manifest.
/// * `tag`: Version to use when transforming local dependencies from `source`.
/// * `dependencies`: List to extend `target` with.
pub fn extend_dependencies_with_version_source<I>(
	target: &mut Manifest,
	source: Manifest,
	template: String,
	tag: Option<String>,
	dependencies: I,
) where
	I: IntoIterator<Item = String>,
{
	let mut target_manifest_workspace = target.workspace.clone().unwrap();
	let source_manifest_workspace = source.workspace.clone().unwrap();

	let updated_dependencies: Vec<_> = dependencies
		.into_iter()
		.filter_map(|k| {
			if let Some(dependency) = source_manifest_workspace.dependencies.get(&k) {
				if let Some(d) = dependency.detail() {
					let mut detail = d.clone();
					if d.path.is_some() {
						detail.path = None;
						detail.git = Some("https://github.com/moondance-labs/tanssi".to_string());
						if let Some(tag) = &tag {
							match tag.as_str() {
								"master" => detail.branch = Some("master".to_string()),
								_ => detail.tag = Some(tag.to_string()),
							}
						};
					}
					Some((k, Dependency::Detailed(Box::from(detail))))
				} else {
					Some((k, dependency.clone()))
				}
			} else {
				None
			}
		})
		.collect();

	target_manifest_workspace.dependencies.extend(updated_dependencies);

	// Update the local runtime dependency.
	let local_runtime = target_manifest_workspace
		.dependencies
		.get_mut(&format!("container-chain-template-{}-runtime", template))
		.unwrap()
		.detail_mut();
	local_runtime.git = None;
	local_runtime.tag = None;
	local_runtime.branch = None;
	local_runtime.path = Some("runtime".to_string());

	target.workspace = Some(target_manifest_workspace);
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

		fn add_workspace_cargo_toml(self) -> Self {
			let workspace_cargo_toml = self
				.workspace
				.as_ref()
				.expect("add_workspace_cargo_toml is only callable if workspace has been created")
				.path()
				.join("Cargo.toml");
			File::create(&workspace_cargo_toml).expect("Failed to create Cargo.toml");
			write(
				&workspace_cargo_toml,
				r#"[workspace]
                resolver = "2"
                members = ["member1"]
                "#,
			)
			.expect("Failed to write Cargo.toml");
			Self { workspace_cargo_toml: Some(workspace_cargo_toml.to_path_buf()), ..self }
		}

		fn add_workspace_cargo_toml_member_not_array(self) -> Self {
			let workspace_cargo_toml = self
				.workspace
				.as_ref()
				.expect("add_workspace_cargo_toml is only callable if workspace has been created")
				.path()
				.join("Cargo.toml");
			File::create(&workspace_cargo_toml).expect("Failed to create Cargo.toml");
			write(
				&workspace_cargo_toml,
				r#"[workspace]
                resolver = "2"
                members = "member1"
                "#,
			)
			.expect("Failed to write Cargo.toml");
			Self { workspace_cargo_toml: Some(workspace_cargo_toml.to_path_buf()), ..self }
		}

		fn add_workspace_cargo_toml_not_defining_workspace(self) -> Self {
			let workspace_cargo_toml = self
				.workspace
				.as_ref()
				.expect("add_workspace_cargo_toml is only callable if workspace has been created")
				.path()
				.join("Cargo.toml");
			File::create(&workspace_cargo_toml).expect("Failed to create Cargo.toml");
			write(&workspace_cargo_toml, r#""#).expect("Failed to write Cargo.toml");
			Self { workspace_cargo_toml: Some(workspace_cargo_toml.to_path_buf()), ..self }
		}

		fn add_outside_workspace_dir(self) -> Self {
			Self { outside_workspace_dir: TempDir::new_in(self.main_tempdir.as_ref()).ok(), ..self }
		}
	}

	#[test]
	fn from_path_works() -> anyhow::Result<(), anyhow::Error> {
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
			.add_workspace_cargo_toml()
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
	}

	#[test]
	fn add_crate_to_workspace_works_well() {
		let test_builder = TestBuilder::default()
			.add_workspace()
			.add_workspace_cargo_toml()
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
				panic!("add_crate_to_workspace fails and should work");
			}
		} else {
			panic!("add_crate_to_workspace fails and should work");
		}
	}

	#[test]
	fn add_crate_to_workspace_fails_if_crate_path_not_inside_workspace() {
		let test_builder = TestBuilder::default()
			.add_workspace()
			.add_workspace_cargo_toml()
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
			.add_workspace_cargo_toml_member_not_array()
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
			.add_workspace_cargo_toml_not_defining_workspace()
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
}
