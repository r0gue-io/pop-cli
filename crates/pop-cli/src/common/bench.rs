// SPDX-License-Identifier: GPL-3.0

use super::binary::{SemanticVersion, which_version};
use crate::{
	cli::traits::*,
	common::binary::{BinaryGenerator, check_and_prompt},
	impl_binary_generator,
};
use pop_chains::omni_bencher_generator;
use std::{
	self,
	cmp::Ordering,
	fs,
	path::{Path, PathBuf},
};

pub(crate) const EXECUTED_COMMAND_COMMENT: &str = "// Executed Command:";
const TARGET_BINARY_VERSION: SemanticVersion = SemanticVersion(0, 11, 1);
const BINARY_NAME: &str = "frame-omni-bencher";

impl_binary_generator!(OmniBencherGenerator, omni_bencher_generator);

/// Checks the status of the `frame-omni-bencher` binary, using the local version if available.
/// If the binary is missing, it is sourced as needed, and if an outdated version exists in cache,
/// the user is prompted to update to the latest release.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_omni_bencher_and_prompt(
	cli: &mut impl Cli,
	spinner: &dyn Spinner,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	Ok(if let Ok(path) = which_version(BINARY_NAME, &TARGET_BINARY_VERSION, &Ordering::Greater) {
		path
	} else {
		source_omni_bencher_binary(cli, spinner, &crate::cache()?, skip_confirm).await?
	})
}

/// Prompt to source the `frame-omni-bencher` binary.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn source_omni_bencher_binary(
	cli: &mut impl Cli,
	spinner: &dyn Spinner,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	check_and_prompt::<OmniBencherGenerator>(cli, spinner, BINARY_NAME, cache_path, skip_confirm)
		.await
}

/// Overwrite the generated weight files' executed command in the destination directory.
///
/// # Arguments
/// * `temp_path`: The path to the temporary directory.
/// * `dest_path`: The path to the destination directory.
/// * `arguments`: The arguments to overwrite the weight directory with.
pub(crate) fn overwrite_weight_dir_command(
	temp_path: &Path,
	dest_path: &Path,
	arguments: &[String],
) -> anyhow::Result<()> {
	// Create the destination directory if it doesn't exist.
	if !dest_path.is_dir() {
		fs::create_dir(dest_path)?;
	}

	// Read and print contents of all files in the temporary directory.
	for entry in temp_path.read_dir()? {
		let path = entry?.path();
		if !path.is_file() {
			continue;
		}

		let destination = dest_path.join(path.file_name().unwrap());
		overwrite_weight_file_command(&path, destination.as_path(), arguments)?;
	}
	Ok(())
}

/// Overwrites the weight file's executed command with the given arguments.
///
/// # Arguments
/// * `temp_file` - The path to the temporary file.
/// * `dest_file` - The path to the destination file.
/// * `arguments` - The arguments to write to the file.
pub(crate) fn overwrite_weight_file_command(
	temp_file: &Path,
	dest_file: &Path,
	arguments: &[String],
) -> anyhow::Result<()> {
	let contents = fs::read_to_string(temp_file)?;
	let lines: Vec<&str> = contents.split("\n").collect();
	let mut iter = lines.iter();
	let mut new_lines: Vec<String> = vec![];

	let mut inside_command_block = false;
	for line in iter.by_ref() {
		if line.starts_with(EXECUTED_COMMAND_COMMENT) {
			inside_command_block = true;
			continue;
		} else if inside_command_block {
			if line.starts_with("//") {
				continue;
			} else if line.trim().is_empty() {
				// Write new command block to the generated weight file.
				new_lines.push(EXECUTED_COMMAND_COMMENT.to_string());
				for argument in arguments {
					new_lines.push(format!("//  {}", argument));
				}
				new_lines.push(String::new());
				break;
			}
		}
		new_lines.push(line.to_string());
	}

	// Write the rest of the file to the destination file.
	for line in iter {
		new_lines.push(line.to_string());
	}

	fs::write(dest_file, new_lines.join("\n"))?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		cli::{MockCli, spinner},
		common::binary::SemanticVersion,
	};
	use fs::File;
	use tempfile::tempdir;

	#[tokio::test]
	async fn source_omni_bencher_binary_works() -> anyhow::Result<()> {
		let cache_path = tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning(format!("⚠️ The {} binary is not found.", BINARY_NAME))
			.expect_confirm("📦 Would you like to source it automatically now?", true)
			.expect_warning(format!("⚠️ The {} binary is not found.", BINARY_NAME));

		let path =
			source_omni_bencher_binary(&mut cli, &spinner(), cache_path.path(), false).await?;
		// Binary path is at least equal to the cache path + "frame-omni-bencher".
		assert!(
			path.to_str()
				.unwrap()
				.starts_with(cache_path.path().join(BINARY_NAME).to_str().unwrap())
		);
		cli.verify()?;

		// Test binary sourcing with skip_confirm = true (no user interaction)
		cli = MockCli::new();

		let path =
			source_omni_bencher_binary(&mut cli, &spinner(), cache_path.path(), true).await?;
		assert!(
			path.to_str()
				.unwrap()
				.starts_with(cache_path.path().join(BINARY_NAME).to_str().unwrap())
		);

		// Verify the downloaded binary version meets the target version requirement
		assert!(
			SemanticVersion::try_from(path.to_str().unwrap().to_string())? >= TARGET_BINARY_VERSION
		);

		cli.verify()
	}

	#[test]
	fn overwrite_weight_dir_command_works() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		let dest_dir = tempdir()?;
		let files = ["weights-1.rs", "weights-2.rs", "weights-3.rs"];

		for file in files {
			let temp_file = temp_dir.path().join(file);
			fs::write(
				temp_file.clone(),
				"// Executed Command:\n// command\n// should\n// be\n// replaced\n\nThis line should not be replaced.",
			)?;
		}

		overwrite_weight_dir_command(
			temp_dir.path(),
			dest_dir.path(),
			&["new".to_string(), "command".to_string(), "replaced".to_string()],
		)?;

		for file in files {
			let dest_file = dest_dir.path().join(file);
			assert_eq!(
				fs::read_to_string(dest_file)?,
				"// Executed Command:\n//  new\n//  command\n//  replaced\n\nThis line should not be replaced."
			);
		}

		Ok(())
	}

	#[test]
	fn overwrite_weight_file_command_works() -> anyhow::Result<()> {
		for (original, expected) in [
			(
				"// Executed Command:\n// command\n// should\n// be\n// replaced\n\nThis line should not be replaced.",
				"// Executed Command:\n//  new\n//  command\n//  replaced\n\nThis line should not be replaced.",
			),
			// Not replace because not "Executed Commnad" comment block found.
			(
				"// command\n// should\n// be\n// replaced\n\nThis line should not be replaced.",
				"// command\n// should\n// be\n// replaced\n\nThis line should not be replaced.",
			),
			// Not replacing contents before the "Executed Command" comment block.
			(
				"Before line should not be replaced\n\n// Executed Command:\n// command\n// should\n// be\n// replaced\n\nAfter line should not be replaced.",
				"Before line should not be replaced\n\n// Executed Command:\n//  new\n//  command\n//  replaced\n\nAfter line should not be replaced.",
			),
		] {
			let temp_dir = tempdir()?;
			let dest_dir = tempdir()?;
			let temp_file = temp_dir.path().join("weights.rs");
			fs::write(temp_file.clone(), original)?;
			let dest_file = dest_dir.path().join("dest_weights.rs");
			File::create(dest_file.clone())?;

			overwrite_weight_file_command(
				&temp_file,
				dest_file.as_path(),
				&["new".to_string(), "command".to_string(), "replaced".to_string()],
			)?;

			let content = fs::read_to_string(dest_file)?;
			assert_eq!(content, expected);
		}
		Ok(())
	}
}
