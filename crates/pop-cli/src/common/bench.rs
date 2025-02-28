// SPDX-License-Identifier: GPL-3.0

use crate::cli::traits::*;
use cliclack::spinner;
use pop_common::{get_relative_or_absolute_path, set_executable_permission};
use pop_parachains::omni_bencher_generator;
use std::{
	env::current_dir,
	path::{Path, PathBuf},
};
use which::which;

/// Checks the status of the `frame-omni-bencher` binary, use the local binary if available.
/// Otherwise, sources it if necessary, and prompts the user to update it if the existing binary in
/// cache is not the latest version.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_omni_bencher_and_prompt(
	cli: &mut impl Cli,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	Ok(match which("frame-omni-bencher") {
		Ok(local_path) => local_path,
		Err(_) => source_omni_bencher_binary(cli, cache_path, skip_confirm).await?,
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
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	let mut binary = omni_bencher_generator(cache_path, None).await?;
	let mut bencher_path = binary.path();
	if !binary.exists() {
		cli.warning("⚠️ The frame-omni-bencher binary is not found.")?;
		let latest = if !skip_confirm {
			cli.confirm("📦 Would you like to source it automatically now?")
				.initial_value(true)
				.interact()?
		} else {
			true
		};
		if latest {
			let spinner = spinner();
			spinner.start("📦 Sourcing frame-omni-bencher...");
			binary.source(false, &(), true).await?;

			spinner.stop(format!(
				"✅ frame-omni-bencher successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			bencher_path = binary.path();
		}
	}

	if binary.stale() {
		cli.warning(format!(
			"ℹ️ There is a newer version of {} available:\n {} -> {}",
			binary.name(),
			binary.version().unwrap_or("None"),
			binary.latest().unwrap_or("None")
		))?;

		let latest = if !skip_confirm {
			cli.confirm(
				"📦 Would you like to source it automatically now? It may take some time..."
					.to_string(),
			)
			.initial_value(true)
			.interact()?
		} else {
			true
		};
		if latest {
			let spinner = spinner();
			spinner.start("📦 Sourcing frame-omni-bencher...");

			binary = omni_bencher_generator(crate::cache()?.as_path(), binary.latest()).await?;
			binary.source(false, &(), true).await?;
			set_executable_permission(binary.path())?;

			spinner.stop(format!(
				"✅ frame-omni-bencher successfully sourced. Cached at: {}",
				binary.path().to_str().unwrap()
			));
			bencher_path = binary.path();
		}
	}
	Ok(bencher_path)
}

/// Get relative path. Returns absolute path if the path is not relative.
pub fn get_relative_path(path: &Path) -> String {
	let cwd = current_dir().unwrap_or(PathBuf::from("./"));
	let path = get_relative_or_absolute_path(cwd.as_path(), path);
	path.as_path().to_str().expect("No path provided").to_string()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::cli::MockCli;

	#[tokio::test]
	async fn source_omni_bencher_binary_works() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli = MockCli::new()
			.expect_warning("⚠️ The frame-omni-bencher binary is not found.")
			.expect_confirm("📦 Would you like to source it automatically now?", true)
			.expect_warning("⚠️ The frame-omni-bencher binary is not found.");

		let path = source_omni_bencher_binary(&mut cli, cache_path.path(), false).await?;
		// Binary path is at least equal to the cache path + "frame-omni-bencher".
		assert!(path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join("frame-omni-bencher").to_str().unwrap()));
		cli.verify()
	}

	#[tokio::test]
	async fn source_omni_bencher_binary_handles_skip_confirm() -> anyhow::Result<()> {
		let cache_path = tempfile::tempdir().expect("Could create temp dir");
		let mut cli =
			MockCli::new().expect_warning("⚠️ The frame-omni-bencher binary is not found.");

		let path = source_omni_bencher_binary(&mut cli, cache_path.path(), true).await?;
		// Binary path is at least equal to the cache path + "frame-omni-bencher".
		assert!(path
			.to_str()
			.unwrap()
			.starts_with(&cache_path.path().join("frame-omni-bencher").to_str().unwrap()));
		cli.verify()
	}
}
