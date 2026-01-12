// SPDX-License-Identifier: GPL-3.0

use super::{Binary, Relay, chain_specs::chain_spec_generator};
use crate::{Error, registry, registry::System, traits::Binary as BinaryT};
use pop_common::sourcing::{filters::prefix, traits::*};
use std::path::Path;

/// Initializes the configuration required to launch a system parachain.
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
) -> Result<Option<super::Chain>, Error> {
	let para = &System;
	let name = para.binary().to_string();
	if command != name {
		return Ok(None);
	}
	// Default to the same version as the relay chain when not explicitly specified
	let source = para
		.source()?
		.resolve(&name, version.or(Some(relay_chain_version)), cache, |f| prefix(f, &name))
		.await
		.into();
	let binary = Binary::Source { name, source, cache: cache.to_path_buf() };
	let chain_spec_generator = match chain {
		Some(chain) => chain_spec_generator(chain, runtime_version, cache).await?,
		None => None,
	};
	Ok(Some(super::Chain { id, binary, chain: chain.map(|c| c.to_string()), chain_spec_generator }))
}

/// Initializes the configuration required to launch a parachain.
///
/// # Arguments
/// * `id` - The parachain identifier.
/// * `command` - The command specified.
/// * `version` - The version of the parachain binary to be used.
/// * `chain` - The chain specified.
/// * `cache` - The cache to be used.
pub(super) async fn from(
	relay: &Relay,
	id: u32,
	command: &str,
	version: Option<&str>,
	chain: Option<&str>,
	cache: &Path,
) -> Result<Option<super::Chain>, Error> {
	if let Some(para) = registry::chains(relay).iter().find(|p| p.binary() == command) {
		let name = para.binary().to_string();
		let source =
			para.source()?.resolve(&name, version, cache, |f| prefix(f, &name)).await.into();
		let binary = Binary::Source { name, source, cache: cache.to_path_buf() };
		return Ok(Some(super::Chain {
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
	use super::*;
	use crate::up::tests::{FALLBACK, RELAY_BINARY_VERSION, SYSTEM_PARA_BINARY_VERSION};
	use pop_common::{
		polkadot_sdk::{sort_by_latest_semantic_version, sort_by_latest_stable_version},
		sourcing::{ArchiveFileSpec, GitHub::ReleaseArchive, Source},
		target,
	};
	use std::path::PathBuf;
	use tempfile::tempdir;

	#[tokio::test]
	async fn system_matches_command() -> anyhow::Result<()> {
		assert!(
			system(
				1000,
				"polkadot",
				None,
				None,
				RELAY_BINARY_VERSION,
				Some("asset-hub-paseo-local"),
				tempdir()?.path()
			)
			.await?
			.is_none()
		);
		Ok(())
	}

	#[tokio::test]
	async fn system_using_relay_version() -> anyhow::Result<()> {
		let expected = &System;
		let para_id = 1000;

		let temp_dir = tempdir()?;
		let parachain = system(
			para_id,
			expected.binary(),
			None,
			None,
			RELAY_BINARY_VERSION,
			None,
			temp_dir.path(),
		)
		.await?
		.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(format!("polkadot-{RELAY_BINARY_VERSION}")),
					tag_pattern: Some("polkadot-{version}".into()),
					prerelease: false,
					version_comparator: sort_by_latest_stable_version,
					fallback: FALLBACK.into(),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![ArchiveFileSpec::new(expected.binary().into(), None, true)],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}).into() && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn system_works() -> anyhow::Result<()> {
		let expected = &System;
		let para_id = 1000;

		let temp_dir = tempdir()?;
		let parachain = system(
			para_id,
			expected.binary(),
			Some(SYSTEM_PARA_BINARY_VERSION),
			None,
			RELAY_BINARY_VERSION,
			None,
			temp_dir.path(),
		)
		.await?
		.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected.binary() && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot".to_string(),
					tag: Some(format!("polkadot-{SYSTEM_PARA_BINARY_VERSION}")),
					tag_pattern: Some("polkadot-{version}".into()),
					prerelease: false,
					version_comparator: sort_by_latest_stable_version,
					fallback: FALLBACK.into(),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![ArchiveFileSpec::new(expected.binary().into(), None, true)],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}).into() && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn system_with_chain_spec_generator_works() -> anyhow::Result<()> {
		let expected = System;
		let runtime_version = "v1.3.3";
		let para_id = 1000;

		let temp_dir = tempdir()?;
		let parachain = system(
			para_id,
			expected.binary(),
			None,
			Some(runtime_version),
			RELAY_BINARY_VERSION,
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
	async fn pop_works() -> anyhow::Result<()> {
		let version = "v0.3.0";
		let expected = "pop-node";
		let para_id = 2000;

		let temp_dir = tempdir()?;
		let parachain =
			from(&Relay::Paseo, para_id, expected, Some(version), None, temp_dir.path())
				.await?
				.unwrap();
		assert_eq!(para_id, parachain.id);
		assert!(matches!(parachain.binary, Binary::Source { name, source, cache }
			if name == expected && source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "pop-node".to_string(),
					tag: Some(format!("node-{version}")),
					tag_pattern: Some("node-{version}".into()),
					prerelease: false,
					version_comparator: sort_by_latest_semantic_version,
					fallback: "v0.3.0".into(),
					archive: format!("{name}-{}.tar.gz", target()?),
					contents: vec![ArchiveFileSpec::new(expected.into(), None, true)],
					latest: parachain.binary.latest().map(|l| l.to_string()),
				}).into() && cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn from_handles_unsupported_command() -> anyhow::Result<()> {
		assert!(
			from(&Relay::Paseo, 2000, "none", None, None, &PathBuf::default())
				.await?
				.is_none()
		);
		Ok(())
	}
}
