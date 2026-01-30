// SPDX-License-Identifier: GPL-3.0

use super::chain_specs::chain_spec_generator;
use crate::{Error, up::chain_specs};
use pop_common::{
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
		TagPattern = "polkadot-{version}",
		Fallback = "stable2512"
	))]
	Polkadot,
}

impl SourceT for &RelayChain {
	type Error = Error;
	/// Defines the source of the binary required for relay chain validators.
	fn source(&self) -> Result<Source, Error> {
		Ok(match self {
			RelayChain::Polkadot => {
				// Source from GitHub release asset
				let repo = GitHub::parse(self.repository())?;
				Source::GitHub(ReleaseArchive {
					owner: repo.org,
					repository: repo.name,
					tag: None,
					tag_pattern: self.tag_pattern().map(|t| t.into()),
					prerelease: false,
					version_comparator: sort_by_latest_stable_version,
					fallback: self.fallback().into(),
					archive: format!("{}-{}.tar.gz", self.binary(), target()?),
					contents: once(self.binary())
						.chain(self.workers())
						.map(|n| ArchiveFileSpec::new(n.into(), None, true))
						.collect(),
					latest: None,
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

/// Initializes the configuration required to launch the relay chain.
///
/// # Arguments
/// * `version` - The version of the relay chain binary to be used.
/// * `runtime_version` - The version of the runtime to be used.
/// * `chain` - The chain specified.
/// * `cache` - The cache to be used.
pub(super) async fn default(
	version: Option<&str>,
	runtime_version: Option<&str>,
	chain: &str,
	cache: &Path,
) -> Result<super::RelayChain, Error> {
	from(RelayChain::Polkadot.binary(), version, runtime_version, chain, cache).await
}

/// Initializes the configuration required to launch the relay chain using the specified command.
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
	chain: &str,
	cache: &Path,
) -> Result<super::RelayChain, Error> {
	if let Some(relay) = RelayChain::VARIANTS
		.iter()
		.find(|r| command.to_lowercase().ends_with(r.binary()))
	{
		let name = relay.binary().to_string();
		let source = relay
			.source()?
			.resolve(&name, version, cache, |f| prefix(f, &name))
			.await
			.into();
		let binary = Binary::Source { name, source, cache: cache.to_path_buf() };
		let runtime = chain_specs::Runtime::from_chain(chain)
			.ok_or(Error::UnsupportedCommand(format!("the relay chain is unsupported: {chain}")))?;
		return Ok(super::RelayChain {
			runtime,
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
	use super::{
		super::tests::{FALLBACK, RELAY_BINARY_VERSION},
		*,
	};
	use pop_common::polkadot_sdk::sort_by_latest_semantic_version;
	use tempfile::tempdir;

	#[tokio::test]
	async fn default_works() -> anyhow::Result<()> {
		let expected = RelayChain::Polkadot;
		let temp_dir = tempdir()?;
		let relay =
			default(Some(RELAY_BINARY_VERSION), None, "paseo-local", temp_dir.path()).await?;
		assert!(matches!(relay.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(format!("polkadot-{RELAY_BINARY_VERSION}")),
					tag_pattern: Some("polkadot-{version}".into()),
					prerelease: false,
					version_comparator: sort_by_latest_stable_version,
					fallback: FALLBACK.into(),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"].map(|b| ArchiveFileSpec::new(b.into(), None, true)).to_vec(),
					latest: relay.binary.latest().map(|l| l.to_string()),
				}).into() && cache == temp_dir.path()
		));
		assert_eq!(relay.workers, expected.workers());
		Ok(())
	}

	#[tokio::test]
	async fn default_with_chain_spec_generator_works() -> anyhow::Result<()> {
		let runtime_version = "v1.3.3";
		let temp_dir = tempdir()?;
		let relay = default(None, Some(runtime_version), "paseo-local", temp_dir.path()).await?;
		assert_eq!(relay.chain, "paseo-local");
		let chain_spec_generator = relay.chain_spec_generator.unwrap();
		assert!(matches!(chain_spec_generator, Binary::Source { name, source, cache }
			if name == "paseo-chain-spec-generator" && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "paseo-runtimes".to_string(),
					tag: Some(runtime_version.to_string()),
					tag_pattern: None,
					prerelease: false,
					version_comparator: sort_by_latest_semantic_version,
					fallback: "v1.4.1".into(),
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: [ArchiveFileSpec::new("chain-spec-generator".into(), Some("paseo-chain-spec-generator".into()), true)].to_vec(),
					latest: chain_spec_generator.latest().map(|l| l.to_string()),
				}).into() && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn from_handles_unsupported_command() -> anyhow::Result<()> {
		assert!(
			matches!(from("none", None, None, "paseo-local", tempdir()?.path()).await, Err(Error::UnsupportedCommand(e))
			if e == "the relay chain command is unsupported: none")
		);
		Ok(())
	}

	#[tokio::test]
	async fn from_handles_local_command() -> anyhow::Result<()> {
		let expected = RelayChain::Polkadot;
		let temp_dir = tempdir()?;
		let relay = from(
			"./bin-stable2512/polkadot",
			Some(RELAY_BINARY_VERSION),
			None,
			"paseo-local",
			temp_dir.path(),
		)
		.await?;
		assert!(matches!(relay.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(format!("polkadot-{RELAY_BINARY_VERSION}")),
					tag_pattern: Some("polkadot-{version}".into()),
					prerelease: false,
					version_comparator: sort_by_latest_stable_version,
					fallback: FALLBACK.into(),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: ["polkadot", "polkadot-execute-worker", "polkadot-prepare-worker"].map(|b| ArchiveFileSpec::new(b.into(), None, true)).to_vec(),
					latest: relay.binary.latest().map(|l| l.to_string()),
				}).into() && cache == temp_dir.path()
		));
		assert_eq!(relay.workers, expected.workers());
		Ok(())
	}
}
