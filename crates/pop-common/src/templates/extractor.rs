// SPDX-License-Identifier: GPL-3.0

use std::{fs, io, path::Path};

use anyhow::Result;

/// Extracts the specified template files from the repository folder to the target folder.
///
/// # Arguments
/// * `template_file` - The name of the template to extract.
/// * `repo_folder` - The path to the repository folder containing the template.
/// * `target_folder` - The destination path where the template files should be copied.
/// * `ignore_folders` - A vector of folder names to ignore during the extraction. If empty, no
///   folders are ignored.
pub fn extract_template_files(
	template_file: String,
	repo_folder: &Path,
	target_folder: &Path,
	ignore_folders: Option<Vec<String>>,
) -> Result<()> {
	let template_folder = repo_folder.join(&template_file);
	if template_folder.is_dir() {
		// Recursively copy all folders and files within. Ignores the specified ones.
		copy_dir_all(&template_folder, target_folder, &ignore_folders.unwrap_or_else(|| vec![]))?;
		return Ok(());
	} else {
		// If not a dir, just copy the file.
		let dst = target_folder.join(&template_file);
		// In case the first file being pulled is not a directory,
		// Make sure the target directory exists.
		fs::create_dir_all(&target_folder)?;
		fs::copy(template_folder, &dst)?;
		Ok(())
	}
}

/// Recursively copy a directory and its files.
///
/// # Arguments
/// * `src`: - The source path of the directory to be copied.
/// * `dst`: - The destination path where the directory and its contents will be copied.
/// * `ignore_folders` - Folders to ignore during the copy process.
fn copy_dir_all(
	src: impl AsRef<Path>,
	dst: impl AsRef<Path>,
	ignore_folders: &Vec<String>,
) -> io::Result<()> {
	fs::create_dir_all(&dst)?;
	for entry in fs::read_dir(src)? {
		let entry = entry?;
		let ty = entry.file_type()?;
		if ty.is_dir() && ignore_folders.contains(&entry.file_name().to_string_lossy().to_string())
		{
			continue;
		} else if ty.is_dir() {
			copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()), ignore_folders)?;
		} else {
			fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use std::fs;

	use anyhow::{Error, Result};

	use super::*;

	fn generate_testing_contract(template: &str) -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir()?;
		let template_folder = temp_dir.path().join(template.to_string());
		fs::create_dir(&template_folder)?;
		fs::File::create(&template_folder.join("lib.rs"))?;
		fs::File::create(&template_folder.join("Cargo.toml"))?;
		fs::create_dir(&temp_dir.path().join("noise_folder"))?;
		fs::create_dir(&template_folder.join("frontend"))?;
		Ok(temp_dir)
	}
	#[test]
	fn extract_template_files_works() -> Result<(), Error> {
		// Contract
		let mut temp_dir = generate_testing_contract("erc20")?;
		let mut output_dir = tempfile::tempdir()?;
		extract_template_files("erc20".to_string(), temp_dir.path(), output_dir.path(), None)?;
		assert!(output_dir.path().join("lib.rs").exists());
		assert!(output_dir.path().join("Cargo.toml").exists());
		assert!(output_dir.path().join("frontend").exists());
		assert!(!output_dir.path().join("noise_folder").exists());
		// ignore the frontend folder
		temp_dir = generate_testing_contract("erc721")?;
		output_dir = tempfile::tempdir()?;
		extract_template_files(
			"erc721".to_string(),
			temp_dir.path(),
			output_dir.path(),
			Some(vec!["frontend".to_string()]),
		)?;
		assert!(output_dir.path().join("lib.rs").exists());
		assert!(output_dir.path().join("Cargo.toml").exists());
		assert!(!output_dir.path().join("frontend").exists());
		assert!(!output_dir.path().join("noise_folder").exists());

		Ok(())
	}
}
