// SPDX-License-Identifier: GPL-3.0

use super::{chain_specs::chain_spec_generator, Binary};
use pop_common::{
	polkadot_sdk::parse_latest_tag,
	sourcing::{
		traits::{Source as _, *},
		GitHub::ReleaseArchive,
		Source,
	},
	target, Error, GitHub,
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
		Fallback = "stable2409"
	))]
	System,
	/// Pop Network makes it easy for smart contract developers to use the power of Polkadot.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/pop-node",
		Binary = "pop-node",
		Prerelease = "false",
		Fallback = "testnet-v0.4.2"
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
				let repo = GitHub::parse(self.repository())?;
				Source::GitHub(ReleaseArchive {
					owner: repo.org,
					repository: repo.name,
					tag,
					tag_format: self.tag_format().map(|t| t.into()),
					archive: format!("{}-{}.tar.gz", self.binary(), target()?),
					contents: vec![(self.binary(), None)],
					latest,
				})
			},
		})
	}
}

impl pop_common::sourcing::traits::Source for Parachain {}

/// Initialises the configuration required to launch a system parachain.
///
/// # Arguments
/// * `id` - The parachain identifier.
/// * `command` - The command specified.
/// * `version` - The version of the parachain binary to be used.
/// * `runtime_version` - The version of the runtime to be used.
/// * `relay_chain_version` - The version of the relay chain binary being used.
/// * `chain` - The chain specified.
/// * `cache` - The cache to be used.
pub(super) async fn system(
	id: u32,
	command: &str,
	version: Option<&str>,
	runtime_version: Option<&str>,
	relay_chain_version: &str,
	chain: Option<&str>,
	cache: &Path,
) -> Result<Option<super::Parachain>, Error> {
	let para = &Parachain::System;
	let name = para.binary();
	if command != name {
		return Ok(None);
	}
	let (tag, latest) = match version {
		Some(version) => (Some(version.to_string()), None),
		None => {
			// Default to same version as relay chain when not explicitly specified
			// Only set latest when caller has not explicitly specified a version to use
			(
				Some(relay_chain_version.to_string()),
				parse_latest_tag(para.releases().await?.iter().map(|s| s.as_str()).collect()),
			)
		},
	};
	let source = TryInto::try_into(para, tag, latest)?;
	let binary = Binary::Source { name: name.to_string(), source, cache: cache.to_path_buf() };
	let chain_spec_generator = match chain {
		Some(chain) => chain_spec_generator(chain, runtime_version, cache).await?,
		None => None,
	};
	Ok(Some(super::Parachain {
		id,
		binary,
		chain: chain.map(|c| c.to_string()),
		chain_spec_generator,
	}))
}

/// Initialises the configuration required to launch a parachain.
///
/// # Arguments
/// * `id` - The parachain identifier.
/// * `command` - The command specified.
/// * `version` - The version of the parachain binary to be used.
/// * `chain` - The chain specified.
/// * `cache` - The cache to be used.
pub(super) async fn from(
	id: u32,
	command: &str,
	version: Option<&str>,
	chain: Option<&str>,
	cache: &Path,
) -> Result<Option<super::Parachain>, Error> {
	if let Some(para) = Parachain::VARIANTS.iter().find(|p| p.binary() == command) {
		let releases = para.releases().await?;
		let tag = Binary::resolve_version(command, version, &releases, cache);
		// Only set latest when caller has not explicitly specified a version to use
		let latest = version.is_none().then(|| releases.first().map(|v| v.to_string())).flatten();
		let binary = Binary::Source {
			name: para.binary().to_string(),
			source: TryInto::try_into(para, tag, latest)?,
			cache: cache.to_path_buf(),
		};
		return Ok(Some(super::Parachain {
			id,
			binary,
			chain: chain.map(|c| c.to_string()),
			chain_spec_generator: None,
		}));
	}
	Ok(None)
}

#[cfg(test)]
mod tests {
	use super::{super::tests::VERSION, *};
	use std::path::PathBuf;
	use tempfile::tempdir;

	#[tokio::test]
	async fn system_matches_command() -> anyhow::Result<()> {
		assert!(system(
			1000,
			"polkadot",
			None,
			None,
			VERSION,
			Some("asset-hub-paseo-local"),
			tempdir()?.path()
		)
		.await?
		.is_none());
		Ok(())
	}

	#[tokio::test]
	async fn system_using_relay_version() -> anyhow::Result<()> {
		let expected = Parachain::System;
		let para_id = 1000;

		let temp_dir = tempdir()?;
		let parachain =
			system(para_id, expected.binary(), None, None, VERSION, None, temp_dir.path())
				.await?
				.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(VERSION.to_string()),
					tag_format: Some("polkadot-{tag}".to_string()),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![(expected.binary(), None)],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn system_works() -> anyhow::Result<()> {
		let expected = Parachain::System;
		let para_id = 1000;

		let temp_dir = tempdir()?;
		let parachain =
			system(para_id, expected.binary(), Some(VERSION), None, VERSION, None, temp_dir.path())
				.await?
				.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(VERSION.to_string()),
					tag_format: Some("polkadot-{tag}".to_string()),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![(expected.binary(), None)],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn system_with_chain_spec_generator_works() -> anyhow::Result<()> {
		let expected = Parachain::System;
		let runtime_version = "v1.3.3";
		let para_id = 1000;

		let temp_dir = tempdir()?;
		let parachain = system(
			para_id,
			expected.binary(),
			None,
			Some(runtime_version),
			"v.13.0",
			Some("asset-hub-paseo-local"),
			temp_dir.path(),
		)
		.await?
		.unwrap();
		assert_eq!(parachain.id, para_id);
		assert_eq!(parachain.chain.unwrap(), "asset-hub-paseo-local");
		let chain_spec_generator = parachain.chain_spec_generator.unwrap();
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
	async fn pop_works() -> anyhow::Result<()> {
		let version = "v1.0";
		let expected = Parachain::Pop;
		let para_id = 2000;

		let temp_dir = tempdir()?;
		let parachain = from(para_id, expected.binary(), Some(version), None, temp_dir.path())
			.await?
			.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "pop-node".to_string(),
					tag: Some(version.to_string()),
					tag_format: None,
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![(expected.binary(), None)],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}) && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn from_handles_unsupported_command() -> anyhow::Result<()> {
		assert!(from(2000, "none", None, None, &PathBuf::default()).await?.is_none());
		Ok(())
	}
}
