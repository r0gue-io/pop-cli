// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use std::{fs, io, path::Path};

/// Extracts the specified template files from the repository folder to the target folder.
///
/// # Arguments
/// * `template_name` - The name of the template to extract.
/// * `repo_folder` - The path to the repository folder containing the template.
/// * `target_folder` - The destination path where the template files should be copied.
///
pub fn extract_template_files(
	template_name: String,
	repo_folder: &Path,
	target_folder: &Path,
) -> Result<()> {
	let template_folder = repo_folder.join(template_name);
	// Recursively copy all folders and files within. Ignores frontend folders.
	copy_dir_all(&template_folder, target_folder)?;
	Ok(())
}

/// Recursively copy a directory and its files.
/// 
/// # Arguments
/// * `src`: - Path to copy from
/// * `dst`: - Path to copy to
///
fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
	fs::create_dir_all(&dst)?;
	for entry in fs::read_dir(src)? {
		let entry = entry?;
		let ty = entry.file_type()?;
		// Ignore frontend folder in templates
		if ty.is_dir() && entry.file_name() == "frontend" {
			continue;
		} else if ty.is_dir() {
			copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
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
		let template_folder = temp_dir.path().join(template.to_string());
		fs::create_dir(&template_folder)?;
		fs::File::create(&template_folder.join("lib.rs"))?;
		fs::File::create(&template_folder.join("Cargo.toml"))?;
		fs::create_dir(&temp_dir.path().join("noise_folder"))?;
		Ok(temp_dir)
	}
	#[test]
	fn extract_template_files_works() -> Result<(), Error> {
		// Contract
		let temp_dir = generate_testing_contract("erc20")?;
		let output_dir = tempfile::tempdir()?;
		extract_template_files("erc20".to_string(), temp_dir.path(), output_dir.path())?;
		assert!(output_dir.path().join("lib.rs").exists());
		assert!(output_dir.path().join("Cargo.toml").exists());
		assert!(!output_dir.path().join("noise_folder").exists());
		assert!(!output_dir.path().join("frontend").exists());

		Ok(())
	}
}
