// SPDX-License-Identifier: GPL-3.0

use super::chain_specs::chain_spec_generator;
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
use std::{iter::once, path::Path};
use strum::VariantArray as _;
use strum_macros::{EnumProperty, VariantArray};

/// A supported relay chain.
#[derive(Debug, EnumProperty, PartialEq, VariantArray)]
pub(super) enum RelayChain {
	/// Polkadot.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot",
		Binary = "polkadot",
		TagFormat = "polkadot-{tag}",
		Fallback = "v1.12.0"
	))]
	Polkadot,
}

impl TryInto for &RelayChain {
	/// Attempt the conversion.
	///
	/// # Arguments
	/// * `tag` - If applicable, a tag used to determine a specific release.
	/// * `latest` - If applicable, some specifier used to determine the latest source.
	fn try_into(&self, tag: Option<String>, latest: Option<String>) -> Result<Source, Error> {
		Ok(match self {
			RelayChain::Polkadot => {
				// Source from GitHub release asset
				let repo = GitHub::parse(self.repository())?;
				Source::GitHub(ReleaseArchive {
					owner: repo.org,
					repository: repo.name,
					tag,
					tag_format: self.tag_format().map(|t| t.into()),
					archive: format!("{}-{}.tar.gz", self.binary(), target()?),
					contents: once(self.binary())
						.chain(self.workers())
						.map(|n| (n, None))
						.collect(),
					latest,
				})
			},
		})
	}
}

impl RelayChain {
	/// The additional worker binaries required for the relay chain.
	fn workers(&self) -> [&'static str; 2] {
		["polkadot-execute-worker", "polkadot-prepare-worker"]
	}
}

impl pop_common::sourcing::traits::Source for RelayChain {}

/// Initialises the configuration required to launch the relay chain.
///
/// # Arguments
/// * `version` - The version of the relay chain binary to be used.
/// * `runtime_version` - The version of the runtime to be used.
/// * `chain` - The chain specified.
/// * `cache` - The cache to be used.
pub(super) async fn default(
	version: Option<&str>,
	runtime_version: Option<&str>,
	chain: Option<&str>,
	cache: &Path,
) -> Result<super::RelayChain, Error> {
	from(RelayChain::Polkadot.binary(), version, runtime_version, chain, cache).await
}

/// Initialises the configuration required to launch the relay chain using the specified command.
///
/// # Arguments
/// * `command` - The command specified.
/// * `version` - The version of the binary to be used.
/// * `runtime_version` - The version of the runtime to be used.
/// * `chain` - The chain specified.
/// * `cache` - The cache to be used.
pub(super) async fn from(
	command: &str,
	version: Option<&str>,
	runtime_version: Option<&str>,
	chain: Option<&str>,
	cache: &Path,
) -> Result<super::RelayChain, Error> {
	if let Some(relay) = RelayChain::VARIANTS
		.iter()
		.find(|r| command.to_lowercase().ends_with(r.binary()))
	{
		let name = relay.binary();
		let releases = relay.releases().await?;
		let tag = Binary::resolve_version(name, version, &releases, cache);
		// Only set latest when caller has not explicitly specified a version to use
		let latest = version.is_none().then(|| releases.first().map(|v| v.to_string())).flatten();
		let binary = Binary::Source {
			name: name.to_string(),
			source: TryInto::try_into(&relay, tag, latest)?,
			cache: cache.to_path_buf(),
		};
		let chain = chain.unwrap_or("rococo-local");
		return Ok(super::RelayChain {
			binary,
			workers: relay.workers(),
			chain: chain.into(),
			chain_spec_generator: chain_spec_generator(chain, runtime_version, cache).await?,
		});
	}

	Err(Error::UnsupportedCommand(format!("the relay chain command is unsupported: {command}")))
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[tokio::test]
	async fn default_works() -> anyhow::Result<()> {
		let expected = RelayChain::Polkadot;
		let version = "v1.12.0";
		let temp_dir = tempdir()?;
		let relay = default(Some(version), None, None, temp_dir.path()).await?;
		assert!(matches!(relay.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(version.to_string()),
					tag_format: Some("polkadot-{tag}".to_string()),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"].map(|b| (b, None)).to_vec(),
					latest: relay.binary.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		assert_eq!(relay.workers, expected.workers());
		Ok(())
	}

	#[tokio::test]
	async fn default_with_chain_spec_generator_works() -> anyhow::Result<()> {
		let runtime_version = "v1.2.7";
		let temp_dir = tempdir()?;
		let relay =
			default(None, Some(runtime_version), Some("paseo-local"), temp_dir.path()).await?;
		assert_eq!(relay.chain, "paseo-local");
		let chain_spec_generator = relay.chain_spec_generator.unwrap();
		assert!(matches!(chain_spec_generator, Binary::Source { name, source, cache }
			if name == "paseo-chain-spec-generator" && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "paseo-runtimes".to_string(),
					tag: Some(runtime_version.to_string()),
					tag_format: None,
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: [("chain-spec-generator", Some("paseo-chain-spec-generator".to_string()))].to_vec(),
					latest: chain_spec_generator.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn from_handles_unsupported_command() -> anyhow::Result<()> {
		assert!(
			matches!(from("none", None, None, None, tempdir()?.path()).await, Err(Error::UnsupportedCommand(e))
			if e == "the relay chain command is unsupported: none")
		);
		Ok(())
	}

	#[tokio::test]
	async fn from_handles_local_command() -> anyhow::Result<()> {
		let expected = RelayChain::Polkadot;
		let version = "v1.12.0";
		let temp_dir = tempdir()?;
		let relay =
			from("./bin-v1.6.0/polkadot", Some(version), None, None, temp_dir.path()).await?;
		assert!(matches!(relay.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(version.to_string()),
					tag_format: Some("polkadot-{tag}".to_string()),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"].map(|b| (b, None)).to_vec(),
					latest: relay.binary.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		assert_eq!(relay.workers, expected.workers());
		Ok(())
	}
}
