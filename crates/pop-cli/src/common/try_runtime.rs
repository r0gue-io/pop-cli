// SPDX-License-Identifier: GPL-3.0

use crate::{
	cli::traits::*,
	common::binary::{check_and_prompt, BinaryGenerator},
	impl_binary_generator,
};
use duct::cmd;
use pop_common::sourcing::Binary;
use pop_parachains::try_runtime_generator;
use std::{
	self,
	path::{Path, PathBuf},
};

const BINARY_NAME: &str = "try-runtime";

impl_binary_generator!(TryRuntimeGenerator, try_runtime_generator);

/// Checks the status of the `try-runtime` binary, using the local version if available.
/// If the binary is missing, it is sourced as needed, and if an outdated version exists in cache,
/// the user is prompted to update to the latest release.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn check_try_runtime_and_prompt(
	cli: &mut impl Cli,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	Ok(match cmd("which", &[BINARY_NAME]).stdout_capture().run() {
		Ok(output) => {
			let path = String::from_utf8(output.stdout)?;
			PathBuf::from(path.trim())
		},
		Err(_) => source_try_runtime_binary(cli, &crate::cache()?, skip_confirm).await?,
	})
}

/// Prompt to source the `try-runtime` binary.
///
/// # Arguments
/// * `cli`: Command line interface.
/// * `cache_path`: The cache directory path.
/// * `skip_confirm`: A boolean indicating whether to skip confirmation prompts.
pub async fn source_try_runtime_binary(
	cli: &mut impl Cli,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	Ok(check_and_prompt::<TryRuntimeGenerator>(cli, BINARY_NAME, cache_path, skip_confirm).await?)
}
