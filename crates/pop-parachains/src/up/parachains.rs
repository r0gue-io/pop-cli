// SPDX-License-Identifier: GPL-3.0
use super::{
	sourcing,
	sourcing::{
		traits::{Source as _, *},
		GitHub::ReleaseArchive,
		Source,
	},
	target, Binary, Error,
};
use std::path::Path;
use strum::VariantArray as _;
use strum_macros::{EnumProperty, VariantArray};

/// A supported parachain.
#[derive(Debug, EnumProperty, PartialEq, VariantArray)]
pub(super) enum Parachain {
	/// Parachain containing core Polkadot protocol features.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot",
		Binary = "polkadot-parachain",
		TagFormat = "polkadot-{tag}",
		Fallback = "v1.12.0"
	))]
	System,
	/// Pop Network makes it easy for smart contract developers to use the power of Polkadot.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/pop-node",
		Binary = "pop-node",
		Prerelease = "true",
		Fallback = "v0.1.0-alpha2"
	))]
	Pop,
}

impl TryInto for Parachain {
	/// Attempt the conversion.
	///
	/// # Arguments
	/// * `tag` - If applicable, a tag used to determine a specific release.
	/// * `latest` - If applicable, some specifier used to determine the latest source.
	fn try_into(&self, tag: Option<String>, latest: Option<String>) -> Result<Source, Error> {
		Ok(match self {
			Parachain::System | Parachain::Pop => {
				// Source from GitHub release asset
				let repo = crate::GitHub::parse(self.repository())?;
				Source::GitHub(ReleaseArchive {
					owner: repo.org,
					repository: repo.name,
					tag,
					tag_format: self.tag_format().map(|t| t.into()),
					archive: format!("{}-{}.tar.gz", self.binary(), target()?),
					contents: vec![self.binary()],
					latest,
				})
			},
		})
	}
}

impl sourcing::traits::Source for Parachain {}

/// Initialises the configuration required to launch a system parachain.
///
/// # Arguments
/// * `id` - The parachain identifier.
/// * `command` - The command specified.
/// * `version` - The version of the parachain binary to be used.
/// * `version` - The version of the relay chain binary being used.
/// * `cache` - The cache to be used.
pub(super) async fn system(
	id: u32,
	command: &str,
	version: Option<&str>,
	relay_chain: &str,
	cache: &Path,
) -> Result<Option<super::Parachain>, Error> {
	let para = &Parachain::System;
	let name = para.binary();
	if command != name {
		return Ok(None);
	}
	let tag = match version {
		Some(version) => Some(version.to_string()),
		None => {
			// Default to same version as relay chain when not explicitly specified
			let version = relay_chain.to_string();
			Some(version)
		},
	};
	let source = TryInto::try_into(para, tag, para.releases().await?.into_iter().nth(0))?;
	let binary = Binary::Source { name: name.to_string(), source, cache: cache.to_path_buf() };
	return Ok(Some(super::Parachain { id, binary }));
}

/// Initialises the configuration required to launch a parachain.
///
/// # Arguments
/// * `id` - The parachain identifier.
/// * `command` - The command specified.
/// * `version` - The version of the parachain binary to be used.
/// * `cache` - The cache to be used.
pub(super) async fn from(
	id: u32,
	command: &str,
	version: Option<&str>,
	cache: &Path,
) -> Result<Option<super::Parachain>, Error> {
	for para in Parachain::VARIANTS.iter().filter(|p| p.binary() == command) {
		let releases = para.releases().await?;
		let tag = Binary::resolve_version(command, version, &releases, cache);
		let binary = Binary::Source {
			name: para.binary().to_string(),
			source: TryInto::try_into(para, tag, releases.iter().nth(0).map(|v| v.to_string()))?,
			cache: cache.to_path_buf(),
		};
		return Ok(Some(super::Parachain { id, binary }));
	}
	Ok(None)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[tokio::test]
	async fn system_matches_command() -> anyhow::Result<()> {
		assert!(system(1000, "polkadot", None, "v1.12.0", tempdir()?.path()).await?.is_none());
		Ok(())
	}

	#[tokio::test]
	async fn system_using_relay_version() -> anyhow::Result<()> {
		let version = "v1.12.0";
		let expected = Parachain::System;
		let para_id = 1000;

		let temp_dir = tempdir()?;
		let parachain = system(para_id, expected.binary(), None, version, temp_dir.path())
			.await?
			.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(version.to_string()),
					tag_format: Some("polkadot-{tag}".to_string()),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![expected.binary()],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn system_works() -> anyhow::Result<()> {
		let version = "v1.12.0";
		let expected = Parachain::System;
		let para_id = 1000;

		let temp_dir = tempdir()?;
		let parachain = system(para_id, expected.binary(), Some(version), version, temp_dir.path())
			.await?
			.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(version.to_string()),
					tag_format: Some("polkadot-{tag}".to_string()),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![expected.binary()],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn pop_works() -> anyhow::Result<()> {
		let version = "v1.0";
		let expected = Parachain::Pop;
		let para_id = 2000;

		let temp_dir = tempdir()?;
		let parachain =
			from(para_id, expected.binary(), Some(version), temp_dir.path()).await?.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "pop-node".to_string(),
					tag: Some(version.to_string()),
					tag_format: None,
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![expected.binary()],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn from_handles_unsupported_command() -> anyhow::Result<()> {
		assert!(from(2000, "none", None, tempdir()?.path()).await?.is_none());
		Ok(())
	}
}
