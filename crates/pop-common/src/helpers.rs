// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use std::{
	collections::HashMap,
	fs,
	io::{Read, Write},
	path::{Path, PathBuf},
	process::Command,
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
pub fn get_project_name_from_path<'a>(path: &'a Path, default: &'a str) -> String {
	path.file_name()
		.and_then(|name| name.to_str())
		.unwrap_or(default)
		.replace("-", "_")
}

/// Returns the relative path from `base` to `full` if `full` is inside `base`.
/// If `full` is outside `base`, returns the absolute path instead.
///
/// # Arguments
/// * `base` - The base directory to compare against.
/// * `full` - The full path to be shortened.
pub fn get_relative_or_absolute_path(base: &Path, full: &Path) -> PathBuf {
	match full.strip_prefix(base) {
		Ok(relative) => relative.to_path_buf(),
		// If prefix is different, return the full path
		Err(_) => full.to_path_buf(),
	}
}

/// Temporarily changes the current working directory while executing a closure.
pub fn with_current_dir<F, R>(dir: &Path, f: F) -> anyhow::Result<R>
where
	F: FnOnce() -> anyhow::Result<R>,
{
	let original_dir = std::env::current_dir()?;
	std::env::set_current_dir(dir)?;
	let result = f();
	std::env::set_current_dir(original_dir)?;
	result
}

/// Temporarily changes the current working directory while executing an asynchronous closure.
pub async fn with_current_dir_async<F, R>(dir: &Path, f: F) -> anyhow::Result<R>
where
	F: AsyncFnOnce() -> anyhow::Result<R>,
{
	let original_dir = std::env::current_dir()?;
	std::env::set_current_dir(dir)?;
	let result = f().await;
	std::env::set_current_dir(original_dir)?;
	result
}

/// Check if the current process is running as root (UID 0).
///
/// Returns `true` if running as root, `false` otherwise.
pub fn is_root() -> bool {
	Command::new("id")
		.arg("-u")
		.output()
		.ok()
		.and_then(|output| String::from_utf8(output.stdout).ok())
		.and_then(|s| s.trim().parse::<u32>().ok())
		.map(|uid| uid == 0)
		.unwrap_or(false)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::command_mock::CommandMock;
	use anyhow::Result;
	use std::{
		fs,
		sync::{Mutex, OnceLock},
	};

	// Changing the current working directory is a global, process-wide side effect.
	// Serialize such tests to avoid flakiness when tests run in parallel.
	static CWD_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

	fn cwd_lock() -> std::sync::MutexGuard<'static, ()> {
		CWD_TEST_MUTEX.get_or_init(|| Mutex::new(())).lock().unwrap()
	}

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
		assert_eq!(get_project_name_from_path(path, "default_name"), "my_parachain");
		Ok(())
	}

	#[test]
	fn get_project_name_from_path_default_value() -> Result<(), Error> {
		let path = Path::new("./");
		assert_eq!(get_project_name_from_path(path, "my-contract"), "my_contract");
		Ok(())
	}

	#[test]
	fn get_relative_or_absolute_path_works() {
		[
			("/path/to/project", "/path/to/project", ""),
			("/path/to/project", "/path/to/src", "/path/to/src"),
			("/path/to/project", "/path/to/project/main.rs", "main.rs"),
			("/path/to/project", "/path/to/project/../main.rs", "../main.rs"),
			("/path/to/project", "/path/to/project/src/main.rs", "src/main.rs"),
		]
		.into_iter()
		.for_each(|(base, full, expected)| {
			assert_eq!(
				get_relative_or_absolute_path(Path::new(base), Path::new(full)),
				Path::new(expected)
			);
		});
	}

	#[test]
	fn with_current_dir_changes_and_restores_cwd() -> anyhow::Result<()> {
		let _guard = cwd_lock();
		let original = std::env::current_dir()?;
		let temp_dir = tempfile::tempdir()?;
		let tmp_path = temp_dir.path().to_path_buf();

		let res: &str = with_current_dir(&tmp_path, || {
			// Inside the closure, the cwd should be the temp dir (canonicalized for macOS /private
			// symlink).
			let cwd = std::env::current_dir().unwrap();
			assert_eq!(cwd.canonicalize().unwrap(), tmp_path.canonicalize().unwrap());
			// Create a file relative to the new cwd to verify it's applied.
			fs::write("hello.txt", b"world").unwrap();
			Ok("done")
		})?;
		assert_eq!(res, "done");

		// After the closure, cwd should be restored.
		assert_eq!(std::env::current_dir()?, original);
		// The file should exist inside the temp dir.
		assert!(tmp_path.join("hello.txt").exists());
		Ok(())
	}

	#[tokio::test]
	async fn with_current_dir_async_changes_and_restores_cwd() -> anyhow::Result<()> {
		// Acquire and drop the mutex guard before async operations
		{
			let _guard = cwd_lock();
		}

		let original = std::env::current_dir()?;
		let temp_dir = tempfile::tempdir()?;
		let tmp_path = temp_dir.path().to_path_buf();

		let res: &str = with_current_dir_async(&tmp_path, || async {
			// Inside the async closure, the cwd should be the temp dir (canonicalized for macOS
			// /private symlink).
			let cwd = std::env::current_dir().unwrap();
			assert_eq!(cwd.canonicalize().unwrap(), tmp_path.canonicalize().unwrap());
			// Create a file relative to the new cwd to verify it's applied.
			fs::write("async.txt", b"ok").unwrap();
			Ok("async-done")
		})
		.await?;
		assert_eq!(res, "async-done");

		// After the closure, cwd should be restored.
		assert_eq!(std::env::current_dir()?, original);
		// The file should exist inside the temp dir.
		assert!(tmp_path.join("async.txt").exists());
		Ok(())
	}

	#[test]
	fn is_root_detects_root_user() {
		CommandMock::default()
			.with_command_script(
				"id",
				r#"#!/bin/sh
echo 0"#,
			)
			.execute_sync(|| {
				assert!(is_root());
			});
	}

	#[test]
	fn is_root_detects_non_root_user() {
		CommandMock::default()
			.with_command_script(
				"id",
				r#"#!/bin/sh
echo 1000"#,
			)
			.execute_sync(|| {
				assert!(!is_root());
			});
	}
}
