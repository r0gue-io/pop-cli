// SPDX-License-Identifier: GPL-3.0

use pop_common::{
	Error,
	git::GitHub,
	polkadot_sdk::sort_by_latest_stable_version,
	sourcing::{
		ArchiveFileSpec, Binary,
		GitHub::*,
		Source,
		filters::prefix,
		traits::{
			Source as SourceT,
			enums::{Source as _, *},
		},
	},
	target,
};
use std::path::PathBuf;
use strum_macros::EnumProperty;

#[derive(Debug, EnumProperty, PartialEq)]
pub(super) enum BenchmarkingCli {
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot",
		Binary = "frame-omni-bencher",
		TagPattern = "polkadot-{version}",
		Fallback = "stable2512"
	))]
	OmniBencher,
}

impl SourceT for BenchmarkingCli {
	type Error = Error;
	/// Defines the source of the binary required for benchmarking.
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
			version_comparator: sort_by_latest_stable_version,
			fallback: self.fallback().into(),
			archive: format!("{binary}-{}.tar.gz", target()?),
			contents: vec![ArchiveFileSpec::new(binary.into(), Some(binary.into()), true)],
			latest: None,
		}))
	}
}

/// Generate the source of the `frame-omni-bencher` binary on the remote repository.
///
/// # Arguments
/// * `cache` - The path to the directory where the binary should be cached.
/// * `version` - An optional version string. If `None`, the latest available version is used.
pub async fn omni_bencher_generator(
	cache: PathBuf,
	version: Option<&str>,
) -> Result<Binary, Error> {
	let cli = BenchmarkingCli::OmniBencher;
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
	async fn omni_bencher_generator_works() -> Result<(), Error> {
		let temp_dir = tempdir()?;
		let temp_dir_path = temp_dir.path().to_path_buf();
		let version = "polkadot-stable2512";
		let binary = omni_bencher_generator(temp_dir_path.clone(), Some(version)).await?;
		assert!(matches!(binary, Binary::Source { name: _, source, cache }
				if source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(version.to_string()),
					tag_pattern: Some("polkadot-{version}".into()),
					prerelease: false,
					version_comparator: sort_by_latest_stable_version,
					fallback: "stable2512".to_string(),
					archive: format!("frame-omni-bencher-{}.tar.gz", target()?),
					contents: ["frame-omni-bencher"].map(|b| ArchiveFileSpec::new(b.into(), Some(b.into()), true)).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}).into() &&
				cache == temp_dir_path.as_path()
		));
		Ok(())
	}
}
