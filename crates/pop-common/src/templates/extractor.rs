// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use std::{fs, io, path::Path};

/// Extracts the specified template files from the repository directory to the target directory.
///
/// # Arguments
/// * `template_name` - The name of the template to extract.
/// * `repo_directory` - The path to the repository directory containing the template.
/// * `target_directory` - The destination path where the template files should be copied.
/// * `ignore_directories` - A vector of directory names to ignore during the extraction. If empty,
///   no directories are ignored.
pub fn extract_template_files(
	template_name: &str,
	repo_directory: &Path,
	target_directory: &Path,
	ignore_directories: Option<Vec<String>>,
) -> Result<()> {
	let template_directory = repo_directory.join(template_name);
	// Recursively copy all directories and files within. Ignores the specified ones.
	copy_dir_all(template_directory, target_directory, &ignore_directories.unwrap_or_default())?;
	Ok(())
}

/// Recursively copy a directory and its files.
///
/// # Arguments
/// * `src`: - The source path of the directory to be copied.
/// * `dst`: - The destination path where the directory and its contents will be copied.
/// * `ignore_directories` - directories to ignore during the copy process.
fn copy_dir_all(
	src: impl AsRef<Path>,
	dst: impl AsRef<Path>,
	ignore_directories: &[String],
) -> io::Result<()> {
	fs::create_dir_all(&dst)?;
	for entry in fs::read_dir(src)? {
		let entry = entry?;
		let ty = entry.file_type()?;
		if ty.is_dir() &&
			ignore_directories.contains(&entry.file_name().to_string_lossy().to_string())
		{
			continue;
		} else if ty.is_dir() {
			copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()), ignore_directories)?;
		} else {
			fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::fs;

	fn generate_testing_contract(template: &str) -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let template_directory = temp_dir.path().join(template.to_string());
		fs::create_dir(&template_directory)?;
		fs::File::create(&template_directory.join("lib.rs"))?;
		fs::File::create(&template_directory.join("Cargo.toml"))?;
		fs::create_dir(&temp_dir.path().join("noise_directory"))?;
		fs::create_dir(&template_directory.join("frontend"))?;
		Ok(temp_dir)
	}
	#[test]
	fn extract_template_files_works() -> Result<(), Error> {
		// Contract
		let mut temp_dir = generate_testing_contract("erc20")?;
		let mut output_dir = tempfile::tempdir()?;
		extract_template_files("erc20", temp_dir.path(), output_dir.path(), None)?;
		assert!(output_dir.path().join("lib.rs").exists());
		assert!(output_dir.path().join("Cargo.toml").exists());
		assert!(output_dir.path().join("frontend").exists());
		assert!(!output_dir.path().join("noise_directory").exists());
		// ignore the frontend directory
		temp_dir = generate_testing_contract("erc721")?;
		output_dir = tempfile::tempdir()?;
		extract_template_files(
			"erc721",
			temp_dir.path(),
			output_dir.path(),
			Some(vec!["frontend".to_string()]),
		)?;
		assert!(output_dir.path().join("lib.rs").exists());
		assert!(output_dir.path().join("Cargo.toml").exists());
		assert!(!output_dir.path().join("frontend").exists());
		assert!(!output_dir.path().join("noise_directory").exists());

		Ok(())
	}
}
