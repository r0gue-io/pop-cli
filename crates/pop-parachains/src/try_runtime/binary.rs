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
pub(super) enum TryRuntimeCli {
	#[strum(props(
		Repository = "https://github.com/r0gue-io/try-runtime-cli",
		Binary = "try-runtime-cli",
		Fallback = "v0.8.0"
	))]
	TryRuntime,
}

impl TryInto for TryRuntimeCli {
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

impl pop_common::sourcing::traits::Source for TryRuntimeCli {}

/// Generate the source of the `try-runtime` binary on the remote repository.
///
/// # Arguments
/// * `cache` - The path to the directory where the binary should be cached.
/// * `version` - An optional version string. If `None`, the latest available version is used.
pub async fn try_runtime_generator(cache: PathBuf, version: Option<&str>) -> Result<Binary, Error> {
	let cli = TryRuntimeCli::TryRuntime;
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
	async fn try_runtime_generator_works() -> Result<(), Error> {
		let temp_dir = tempdir()?.into_path();
		let version = "v0.8.0";
		let binary = try_runtime_generator(temp_dir.clone(), None).await?;
		assert!(matches!(binary, Binary::Source { name: _, source, cache }
				if source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "try-runtime-cli".to_string(),
					tag: Some(version.to_string()),
					tag_format: None,
					archive: format!("try-runtime-cli-{}.tar.gz", target()?),
					contents: ["try-runtime-cli"].map(|b| (b, Some(b.to_string()))).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}) &&
				cache == temp_dir.as_path()
		));
		Ok(())
	}
}
