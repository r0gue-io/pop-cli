// SPDX-License-Identifier: GPL-3.0

pub use pop_common::{
	git::GitHub,
	sourcing::{
		traits::{Source as _, *},
		Binary,
		GitHub::*,
		Source,
	},
	target, Error,
};
use std::path::PathBuf;
use strum_macros::EnumProperty;

#[derive(Debug, EnumProperty, PartialEq)]
pub(super) enum BenchmarkingCli {
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot",
		Binary = "frame-omni-bencher",
		Fallback = "polkadot-stable2412"
	))]
	OmniBencher,
}

impl TryInto for BenchmarkingCli {
	/// Attempt the conversion.
	///
	/// # Arguments
	/// * `tag` - If applicable, a tag used to determine a specific release.
	/// * `latest` - If applicable, some specifier used to determine the latest source.
	fn try_into(
		&self,
		tag: Option<String>,
		latest: Option<String>,
	) -> Result<pop_common::sourcing::Source, Error> {
		// Source from GitHub release asset
		let repo = GitHub::parse(self.repository())?;
		let binary = self.binary();
		Ok(Source::GitHub(ReleaseArchive {
			owner: repo.org,
			repository: repo.name,
			tag,
			tag_format: self.tag_format().map(|t| t.into()),
			archive: format!("{binary}-{}.tar.gz", target()?),
			contents: vec![(binary, Some(binary.to_string()))],
			latest,
		}))
	}
}

impl pop_common::sourcing::traits::Source for BenchmarkingCli {}

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
	let name = cli.binary();
	let releases = cli.releases().await?;
	let tag = Binary::resolve_version(name, version, &releases, &cache);
	// Only set latest when caller has not explicitly specified a version to use
	let latest = version.is_none().then(|| releases.first().map(|v| v.to_string())).flatten();
	let binary = Binary::Source {
		name: name.to_string(),
		source: TryInto::try_into(&cli, tag, latest)?,
		cache: cache.to_path_buf(),
	};
	Ok(binary)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[tokio::test]
	async fn omni_bencher_generator_works() -> Result<(), Error> {
		let temp_dir = tempdir()?;
		let temp_dir_path = temp_dir.into_path();
		let version = "polkadot-stable2412";
		let binary = omni_bencher_generator(temp_dir_path.clone(), Some(version)).await?;
		assert!(matches!(binary, Binary::Source { name: _, source, cache }
				if source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(version.to_string()),
					tag_format: None,
					archive: format!("frame-omni-bencher-{}.tar.gz", target()?),
					contents: ["frame-omni-bencher"].map(|b| (b, Some(b.to_string()))).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}) &&
				cache == temp_dir_path.as_path()
		));
		Ok(())
	}
}
