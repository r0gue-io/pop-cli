// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use regex::Regex;
use std::{
	collections::HashMap,
	fs,
	io::{Read, Write},
	path::{Component, Path, PathBuf},
};

/// Replaces occurrences of specified strings in a file with new values.
///
/// # Arguments
///
/// * `file_path` - A `PathBuf` specifying the path to the file to be modified.
/// * `replacements` - A `HashMap` where each key-value pair represents a target string and its
///   corresponding replacement string.
pub fn replace_in_file(file_path: PathBuf, replacements: HashMap<&str, &str>) -> Result<(), Error> {
	// Read the file content
	let mut file_content = String::new();
	fs::File::open(&file_path)?.read_to_string(&mut file_content)?;
	// Perform the replacements
	let mut modified_content = file_content;
	for (target, replacement) in &replacements {
		modified_content = modified_content.replace(target, replacement);
	}
	// Write the modified content back to the file
	let mut file = fs::File::create(&file_path)?;
	file.write_all(modified_content.as_bytes())?;
	Ok(())
}

/// Gets the last component (name of a project) of a path or returns a default value if the path has
/// no valid last component.
///
/// # Arguments
/// * `path` - Location path of the project.
/// * `default` - The default string to return if the path has no valid last component.
pub fn get_project_name_from_path<'a>(path: &'a Path, default: &'a str) -> &'a str {
	path.file_name().and_then(|name| name.to_str()).unwrap_or(default)
}

/// Transforms a path without prefix into a relative path starting at the current directory.
///
/// # Arguments
/// * `path` - The path to be prefixed if needed.
pub fn prefix_with_current_dir_if_needed(path: PathBuf) -> PathBuf {
	let components = &path.components().collect::<Vec<Component>>();
	if !components.is_empty() {
		// If the first component is a normal component, we prefix the path with the current dir
		if let Component::Normal(_) = components[0] {
			return <Component<'_> as AsRef<Path>>::as_ref(&Component::CurDir).join(path);
		}
	}
	path
}

/// Parses a list of tags to identify the latest one, prioritizing tags in the stable format
/// first.
pub fn parse_latest_tag(tags: Vec<&str>) -> Option<String> {
	match parse_stable_format(tags.clone()) {
		Some(last_stable_tag) => Some(last_stable_tag),
		None => parse_version_format(tags),
	}
}

/// Parse the stable release tags.
fn parse_stable_format(tags: Vec<&str>) -> Option<String> {
	// Regex for polkadot-stableYYMM and polkadot-stableYYMM-X
	let stable_reg = Regex::new(
		r"(polkadot-(parachain-)?)?stable(?P<year>\d{2})(?P<month>\d{2})(-(?P<patch>\d+))?(-rc\d+)?",
	)
	.expect("Valid regex");
	tags.into_iter()
		.filter_map(|tag| {
			// Skip the pre-release label
			if tag.contains("-rc") {
				return None;
			}
			stable_reg.captures(tag).and_then(|v| {
				let year = v.name("year")?.as_str().parse::<u32>().ok()?;
				let month = v.name("month")?.as_str().parse::<u32>().ok()?;
				let patch =
					v.name("patch").and_then(|m| m.as_str().parse::<u32>().ok()).unwrap_or(0);
				Some((tag, (year, month, patch)))
			})
		})
		.max_by(|a, b| {
			let (_, (year_a, month_a, patch_a)) = a;
			let (_, (year_b, month_b, patch_b)) = b;
			// Compare by year, then by month, then by patch number
			year_a
				.cmp(year_b)
				.then_with(|| month_a.cmp(month_b))
				.then_with(|| patch_a.cmp(patch_b))
		})
		.map(|(tag_str, _)| tag_str.to_string())
}

/// Parse the versioning release tags.
fn parse_version_format(tags: Vec<&str>) -> Option<String> {
	// Regex for polkadot-vmajor.minor.patch format
	let version_reg = Regex::new(r"v(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)(-rc\d+)?")
		.expect("Valid regex");
	tags.into_iter()
		.filter_map(|tag| {
			// Skip the pre-release label
			if tag.contains("-rc") {
				return None;
			}
			version_reg.captures(tag).and_then(|v| {
				let major = v.name("major")?.as_str().parse::<u32>().ok()?;
				let minor = v.name("minor")?.as_str().parse::<u32>().ok()?;
				let patch = v.name("patch")?.as_str().parse::<u32>().ok()?;
				Some((tag, (major, minor, patch)))
			})
		})
		.max_by_key(|&(_, version)| version)
		.map(|(tag_str, _)| tag_str.to_string())
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::fs;

	#[test]
	fn test_replace_in_file() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		let file_path = temp_dir.path().join("file.toml");
		let mut file = fs::File::create(temp_dir.path().join("file.toml"))?;
		writeln!(file, "name = test, version = 5.0.0")?;
		let mut replacements_in_cargo = HashMap::new();
		replacements_in_cargo.insert("test", "changed_name");
		replacements_in_cargo.insert("5.0.0", "5.0.1");
		replace_in_file(file_path.clone(), replacements_in_cargo)?;
		let content = fs::read_to_string(file_path).expect("Could not read file");
		assert_eq!(content.trim(), "name = changed_name, version = 5.0.1");
		Ok(())
	}

	#[test]
	fn get_project_name_from_path_works() -> Result<(), Error> {
		let path = Path::new("./path/to/project/my-parachain");
		assert_eq!(get_project_name_from_path(path, "default_name"), "my-parachain");
		Ok(())
	}

	#[test]
	fn get_project_name_from_path_default_value() -> Result<(), Error> {
		let path = Path::new("./");
		assert_eq!(get_project_name_from_path(path, "my-contract"), "my-contract");
		Ok(())
	}

	#[test]
	fn prefix_with_current_dir_if_needed_works_well() {
		let no_prefixed_path = PathBuf::from("my/path".to_string());
		let current_dir_prefixed_path = PathBuf::from("./my/path".to_string());
		let parent_dir_prefixed_path = PathBuf::from("../my/path".to_string());
		let root_dir_prefixed_path = PathBuf::from("/my/path".to_string());
		let empty_path = PathBuf::from("".to_string());

		assert_eq!(
			prefix_with_current_dir_if_needed(no_prefixed_path),
			PathBuf::from("./my/path/".to_string())
		);
		assert_eq!(
			prefix_with_current_dir_if_needed(current_dir_prefixed_path),
			PathBuf::from("./my/path/".to_string())
		);
		assert_eq!(
			prefix_with_current_dir_if_needed(parent_dir_prefixed_path),
			PathBuf::from("../my/path/".to_string())
		);
		assert_eq!(
			prefix_with_current_dir_if_needed(root_dir_prefixed_path),
			PathBuf::from("/my/path/".to_string())
		);
		assert_eq!(prefix_with_current_dir_if_needed(empty_path), PathBuf::from("".to_string()));
	}

	#[test]
	fn parse_latest_tag_works() {
		let mut tags = vec![];
		assert_eq!(parse_latest_tag(tags), None);
		tags = vec![
			"polkadot-stable2409",
			"polkadot-stable2409-1",
			"polkadot-stable2407",
			"polkadot-v1.10.0",
			"polkadot-v1.11.0",
			"polkadot-v1.12.0",
			"polkadot-v1.7.0",
			"polkadot-v1.8.0",
			"polkadot-v1.9.0",
			"v1.15.1-rc2",
		];
		assert_eq!(parse_latest_tag(tags), Some("polkadot-stable2409-1".to_string()));
	}

	#[test]
	fn parse_stable_format_works() {
		let mut tags = vec![];
		assert_eq!(parse_stable_format(tags), None);
		tags = vec!["polkadot-stable2407", "polkadot-stable2408"];
		assert_eq!(parse_stable_format(tags), Some("polkadot-stable2408".to_string()));
		tags = vec!["polkadot-stable2407", "polkadot-stable2501"];
		assert_eq!(parse_stable_format(tags), Some("polkadot-stable2501".to_string()));
		// Skip the pre-release label
		tags = vec!["polkadot-stable2407", "polkadot-stable2407-1", "polkadot-stable2407-1-rc1"];
		assert_eq!(parse_stable_format(tags), Some("polkadot-stable2407-1".to_string()));
	}

	#[test]
	fn parse_version_format_works() {
		let mut tags: Vec<&str> = vec![];
		assert_eq!(parse_version_format(tags), None);
		tags = vec![
			"polkadot-v1.10.0",
			"polkadot-v1.11.0",
			"polkadot-v1.12.0",
			"polkadot-v1.7.0",
			"polkadot-v1.8.0",
			"polkadot-v1.9.0",
		];
		assert_eq!(parse_version_format(tags), Some("polkadot-v1.12.0".to_string()));
		tags = vec!["v1.0.0", "v2.0.0", "v3.0.0"];
		assert_eq!(parse_version_format(tags), Some("v3.0.0".to_string()));
		// Skip the pre-release label
		tags = vec!["polkadot-v1.12.0", "v1.15.1-rc2"];
		assert_eq!(parse_version_format(tags), Some("polkadot-v1.12.0".to_string()));
	}
}
