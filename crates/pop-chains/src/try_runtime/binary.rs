// SPDX-License-Identifier: GPL-3.0

use pop_common::{
	git::GitHub,
	polkadot_sdk::sort_by_latest_semantic_version,
	sourcing::{
		filters::prefix,
		traits::{
			enums::{Source as _, *},
			Source as SourceT,
		},
		ArchiveFileSpec, Binary,
		GitHub::*,
		Source,
	},
	target, Error,
};
use std::path::PathBuf;
use strum_macros::EnumProperty;

#[derive(Debug, EnumProperty, PartialEq)]
pub(super) enum TryRuntimeCli {
	#[strum(props(
		Repository = "https://github.com/r0gue-io/try-runtime-cli",
		Binary = "try-runtime-cli",
		Fallback = "v0.8.0"
	))]
	TryRuntime,
}

impl SourceT for TryRuntimeCli {
	type Error = Error;
	/// Defines the source of the binary required for testing runtime upgrades.
	fn source(&self) -> Result<Source, Error> {
		// Source from GitHub release asset
		let repo = GitHub::parse(self.repository())?;
		let binary = self.binary();
		Ok(Source::GitHub(ReleaseArchive {
			owner: repo.org,
			repository: repo.name,
			tag: None,
			tag_pattern: self.tag_pattern().map(|t| t.into()),
			prerelease: false,
			version_comparator: sort_by_latest_semantic_version,
			fallback: self.fallback().into(),
			archive: format!("{binary}-{}.tar.gz", target()?),
			contents: vec![ArchiveFileSpec::new(binary.into(), Some(binary.into()), true)],
			latest: None,
		}))
	}
}

/// Generate the source of the `try-runtime` binary on the remote repository.
///
/// # Arguments
/// * `cache` - The path to the directory where the binary should be cached.
/// * `version` - An optional version string. If `None`, the latest available version is used.
pub async fn try_runtime_generator(cache: PathBuf, version: Option<&str>) -> Result<Binary, Error> {
	let cli = TryRuntimeCli::TryRuntime;
	let name = cli.binary().to_string();
	let source = cli
		.source()?
		.resolve(&name, version, cache.as_path(), |f| prefix(f, &name))
		.await
		.into();
	let binary = Binary::Source { name, source, cache: cache.to_path_buf() };
	Ok(binary)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[tokio::test]
	async fn try_runtime_generator_works() -> Result<(), Error> {
		let temp_dir = tempdir()?;
		let path = temp_dir.path().to_path_buf();
		let version = "v0.8.0";
		let binary = try_runtime_generator(path.clone(), None).await?;
		assert!(matches!(binary, Binary::Source { name: _, source, cache }
				if source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "try-runtime-cli".to_string(),
					tag: Some(version.to_string()),
					tag_pattern: None,
					prerelease: false,
					version_comparator: sort_by_latest_semantic_version,
					fallback: version.into(),
					archive: format!("try-runtime-cli-{}.tar.gz", target()?),
					contents: ["try-runtime-cli"].map(|b| ArchiveFileSpec::new(b.into(), Some(b.into()), true)).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}).into() &&
				cache == path
		));
		Ok(())
	}
}
