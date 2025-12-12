// SPDX-License-Identifier: GPL-3.0

use crate::{Error, registry::traits};
use pop_common::{
	git::GitHub,
	polkadot_sdk::sort_by_latest_semantic_version,
	sourcing::{
		ArchiveFileSpec, ArchiveType,
		GitHub::*,
		Source, SourcedArchive,
		filters::prefix,
		traits::{
			Source as SourceT,
			enums::{Source as _, *},
		},
	},
	target,
};
use std::path::{Path, PathBuf};
use strum::{EnumProperty as _, VariantArray as _};
use strum_macros::{AsRefStr, EnumProperty, VariantArray};

/// A supported runtime.
#[repr(u8)]
#[derive(AsRefStr, Clone, Debug, EnumProperty, Eq, Hash, PartialEq, VariantArray)]
pub enum Runtime {
	/// Kusama.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot-runtimes",
		Binary = "chain-spec-generator",
		Chain = "kusama-local",
		Fallback = "v1.4.1"
	))]
	Kusama = 0,
	/// Paseo.
	#[strum(props(
		Repository = "https://github.com/paseo-network/runtimes",
		File = "paseo-local",
		Chain = "paseo-local",
		Fallback = "v2.0.2"
	))]
	Paseo = 1,
	/// Polkadot.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot-runtimes",
		Binary = "chain-spec-generator",
		Chain = "polkadot-local",
		Fallback = "v1.4.1"
	))]
	Polkadot = 2,
	/// Westend.
	#[strum(props(Repository = "https://github.com/r0gue-io/polkadot", Chain = "westend-local",))]
	Westend = 3,
}

impl SourceT for Runtime {
	type Error = Error;
	/// Defines the source of the binary required for generating chain specifications.
	fn source(&self) -> Result<Source, Error> {
		// Source from GitHub release asset
		let repo = GitHub::parse(self.repository())?;
		let name = self.name().to_lowercase();
		match (self.binary(), self.file()) {
			(Ok(binary), Err(_)) => Ok(Source::GitHub(ReleaseArchive {
				owner: repo.org,
				repository: repo.name,
				tag: None,
				tag_pattern: self.tag_pattern().map(|t| t.into()),
				prerelease: false,
				version_comparator: sort_by_latest_semantic_version,
				fallback: self.fallback().into(),
				archive: format!("{binary}-{}.tar.gz", target()?),
				contents: vec![ArchiveFileSpec::new(
					binary.into(),
					Some(format!("{name}-{binary}").into()),
					true,
				)],
				latest: None,
			})),
			(Err(_), Ok(file)) => Ok(Source::GitHub(ReleaseArchive {
				owner: repo.org,
				repository: repo.name,
				tag: None,
				tag_pattern: self.tag_pattern().map(|t| t.into()),
				prerelease: false,
				version_comparator: sort_by_latest_semantic_version,
				fallback: self.fallback().into(),
                archive: format!("{file}.json"),
				contents: vec![ArchiveFileSpec::new(
					format!("{file}.json"),
					Some(file.to_string().into()),
					true,
				)],
				latest: None,
			})),
			_ => Err(Error::Config(
				"Runtime sourcing for chain specs can only contains the chain spec generator or the chain spec file".to_owned(),
			)),
		}
	}
}

impl Runtime {
	/// Converts an underlying discriminator value to a relay chain [Runtime].
	///
	/// # Arguments
	/// * `value` - The discriminator value to be converted.
	pub fn from(value: u8) -> Option<Self> {
		match value {
			0 => Some(Self::Kusama),
			1 => Some(Self::Paseo),
			2 => Some(Self::Polkadot),
			3 => Some(Self::Westend),
			_ => None,
		}
	}

	/// Parses a [Runtime] from its chain identifier.
	///
	/// # Arguments
	/// * `chain` - The chain identifier.
	pub fn from_chain(chain: &str) -> Option<Self> {
		Runtime::VARIANTS
			.iter()
			.find(|r| chain.to_lowercase().ends_with(r.chain()))
			.cloned()
	}

	/// The chain spec identifier.
	pub(crate) fn chain(&self) -> &'static str {
		self.get_str("Chain").expect("expected specification of `Chain`")
	}

	/// The name of the runtime.
	pub fn name(&self) -> &str {
		self.as_ref()
	}

	/// Returns the rollups registered for the relay chain.
	pub fn rollups(&self) -> &'static [Box<dyn traits::Rollup>] {
		crate::registry::rollups(self)
	}
}

pub(super) async fn chain_spec_generator(
	chain: &str,
	version: Option<&str>,
	cache: &Path,
) -> Result<Option<SourcedArchive>, Error> {
	if let Some(runtime) = Runtime::from_chain(chain) {
		if runtime == Runtime::Westend {
			// Westend runtimes included with binary.
			return Ok(None);
		}

		let binary_name = if let Ok(binary) = runtime.binary() {
			binary
		} else {
			return Ok(None);
		};
		let name = format!("{}-{}", runtime.name().to_lowercase(), binary_name);
		let source = runtime
			.source()?
			.resolve(&name, version, cache, |f| prefix(f, &name))
			.await
			.into();
		let binary = SourcedArchive::Source {
			name,
			source,
			cache: cache.to_path_buf(),
			archive_type: ArchiveType::Binary,
		};
		return Ok(Some(binary));
	}
	Ok(None)
}

pub(super) async fn chain_spec_file(
	chain: &str,
	version: Option<&str>,
	cache: &Path,
) -> Result<Option<SourcedArchive>, Error> {
	if let Some(runtime) = Runtime::from_chain(chain) {
		let file = if let Ok(file) = runtime.file() {
			file
		} else {
			return Ok(None);
		};

		// The File prop name is only valid for the relay chains, we need to use the right
		// parachain name for parachains chain specs (differently of chain-spec-generator which was
		// unique for all the chains)
		let mut name = if chain.contains(file) {
			chain.to_owned()
		} else {
			format!("{}-{}", runtime.name().to_lowercase(), file)
		};

		let mut source = runtime.source()?;

		// In case the File prop isn't the source archive to download (parachain case), we need to
		// update the source.
		if let Source::GitHub(ReleaseArchive { ref mut archive, ref mut contents, .. }) = source &&
			let Some(&mut ArchiveFileSpec {
				name: ref mut contents_name, ref mut target, ..
			}) = contents.first_mut()
		{
			*target = Some(PathBuf::from(name.clone()));
			if let Some(file_extension) =
				Path::new(&contents_name.clone()).extension().and_then(|ext| ext.to_str())
			{
				*archive = name.clone() + "." + file_extension;
				*contents_name = name.clone() + "." + file_extension;
				name = name.clone() + "." + file_extension;
			} else {
				*archive = name.clone();
				*contents_name = name.clone();
			}
		}

		let source: Box<Source> =
			source.resolve(&name, version, cache, |f| prefix(f, &name)).await.into();

		let chain_spec_file = SourcedArchive::Source {
			name,
			source,
			cache: cache.to_path_buf(),
			archive_type: ArchiveType::File,
		};
		return Ok(Some(chain_spec_file));
	}
	Ok(None)
}

#[cfg(test)]
mod tests {
	use super::*;
	use Runtime::*;
	use tempfile::tempdir;

	#[tokio::test]
	async fn kusama_works() -> anyhow::Result<()> {
		let expected = Runtime::Kusama;
		let version = "v1.4.1".to_string();
		let temp_dir = tempdir()?;
		let binary = chain_spec_generator("kusama-local", Some(&version), temp_dir.path())
			.await?
			.unwrap();
		assert!(matches!(binary, SourcedArchive::Source { name, source, cache, archive_type }
			if name == format!("{}-{}", expected.name().to_lowercase(), expected.binary().unwrap()) &&
				source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot-runtimes".to_string(),
					tag: Some(version),
					tag_pattern: None,
					prerelease: false,
					version_comparator: sort_by_latest_semantic_version,
					fallback: expected.fallback().to_string(),
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: ["chain-spec-generator"].map(|b| ArchiveFileSpec::new(b.into(), Some(format!("kusama-{b}").into()), true)).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}).into() &&
				cache == temp_dir.path() && archive_type == ArchiveType::Binary
		));
		Ok(())
	}

	#[tokio::test]
	async fn paseo_works() -> anyhow::Result<()> {
		let expected = Runtime::Paseo;
		let version = "v2.0.2";
		let temp_dir = tempdir()?;
		let file = chain_spec_file("paseo-local", Some(version), temp_dir.path()).await?.unwrap();
		assert!(matches!(file, SourcedArchive::Source { name, source, cache, archive_type }
			if name == "paseo-local.json" &&
				archive_type == ArchiveType::File &&
				matches!(*source, Source::GitHub(ReleaseArchive {
					ref owner,
					ref repository,
					ref tag,
					ref tag_pattern,
					prerelease,
					ref fallback,
					ref archive,
					ref contents,
					..
				}) if owner == "paseo-network" &&
					repository == "runtimes" &&
					tag == &Some(version.to_string()) &&
					tag_pattern.is_none() &&
					!prerelease &&
					fallback == expected.fallback() &&
					archive == "paseo-local.json" &&
					contents == &vec![ArchiveFileSpec::new(
						"paseo-local.json".to_string(),
						Some("paseo-local".into()),
						true
					)]
				) &&
				cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn polkadot_works() -> anyhow::Result<()> {
		let expected = Runtime::Polkadot;
		let version = "v1.4.1";
		let temp_dir = tempdir()?;
		let binary = chain_spec_generator("polkadot-local", Some(version), temp_dir.path())
			.await?
			.unwrap();
		assert!(matches!(binary, SourcedArchive::Source { name, source, cache, archive_type }
			if name == format!("{}-{}", expected.name().to_lowercase(), expected.binary().unwrap()) &&
				source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot-runtimes".to_string(),
					tag: Some(version.to_string()),
					tag_pattern: None,
					prerelease: false,
					version_comparator: sort_by_latest_semantic_version,
					fallback: expected.fallback().to_string(),
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: ["chain-spec-generator"].map(|b| ArchiveFileSpec::new(b.into(), Some(format!("polkadot-{b}").into()), true)).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}).into() &&
				cache == temp_dir.path() &&
				archive_type == ArchiveType::Binary
		));
		Ok(())
	}

	#[tokio::test]
	async fn chain_spec_generator_returns_none_when_no_match() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		assert_eq!(chain_spec_generator("rococo-local", None, temp_dir.path()).await?, None);
		Ok(())
	}

	#[tokio::test]
	async fn chain_spec_generator_returns_none_when_westend() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		assert_eq!(chain_spec_generator("westend-local", None, temp_dir.path()).await?, None);
		Ok(())
	}

	#[test]
	fn from_u8_works() {
		for i in 0u8..4 {
			assert_eq!(Runtime::from(i).unwrap() as u8, i);
		}
		assert_eq!(Runtime::from(4), None);
	}

	#[test]
	fn from_chain_works() {
		for (chain, expected) in [
			("kusama-local", Kusama),
			("paseo-local", Paseo),
			("polkadot-local", Polkadot),
			("westend-local", Westend),
		] {
			assert_eq!(Runtime::from_chain(chain).unwrap(), expected);
		}
		assert_eq!(Runtime::from_chain("pop"), None);
	}

	#[test]
	fn rollups_works() {
		let comparator = |rollups: &[Box<dyn traits::Rollup>]| {
			rollups
				.iter()
				.map(|r| (r.id(), r.chain().to_string(), r.name().to_string()))
				.collect::<Vec<_>>()
		};
		{};
		for runtime in Runtime::VARIANTS {
			assert_eq!(
				comparator(runtime.rollups()),
				comparator(crate::registry::rollups(runtime))
			);
		}
	}

	// Tests for chain_spec_file function

	#[tokio::test]
	async fn chain_spec_file_paseo_relay_works() -> anyhow::Result<()> {
		let expected = Runtime::Paseo;
		let version = "v2.0.2";
		let chain = "paseo-local";
		let temp_dir = tempdir()?;

		let file = chain_spec_file(chain, Some(version), temp_dir.path()).await?.unwrap();

		assert!(matches!(file, SourcedArchive::Source { name, source, cache, archive_type }
			if name == "paseo-local.json" &&
				archive_type == ArchiveType::File &&
				matches!(*source, Source::GitHub(ReleaseArchive {
					ref owner,
					ref repository,
					ref tag,
					ref tag_pattern,
					prerelease,
					ref fallback,
					ref archive,
					ref contents,
					..
				}) if owner == "paseo-network" &&
					repository == "runtimes" &&
					tag == &Some(version.to_string()) &&
					tag_pattern.is_none() &&
					!prerelease &&
					fallback == expected.fallback() &&
					archive == "paseo-local.json" &&
					contents == &vec![ArchiveFileSpec::new(
						"paseo-local.json".to_string(),
						Some("paseo-local".into()),
						true
					)]
				) &&
				cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn chain_spec_file_paseo_parachain_works() -> anyhow::Result<()> {
		let expected = Runtime::Paseo;
		let version = "v2.0.2";
		let chain = "asset-hub-paseo-local";
		let temp_dir = tempdir()?;

		let file = chain_spec_file(chain, Some(version), temp_dir.path()).await?.unwrap();

		assert!(matches!(file, SourcedArchive::Source { name, source, cache, archive_type }
			if name == "asset-hub-paseo-local.json" &&
				archive_type == ArchiveType::File &&
				matches!(*source, Source::GitHub(ReleaseArchive {
					ref owner,
					ref repository,
					ref tag,
					ref tag_pattern,
					prerelease,
					ref fallback,
					ref archive,
					ref contents,
					..
				}) if owner == "paseo-network" &&
					repository == "runtimes" &&
					tag == &Some(version.to_string()) &&
					tag_pattern.is_none() &&
					!prerelease &&
					fallback == expected.fallback() &&
					archive == "asset-hub-paseo-local.json" &&
					contents == &vec![ArchiveFileSpec::new(
						"asset-hub-paseo-local.json".to_string(),
						Some("asset-hub-paseo-local".into()),
						true
					)]
				) &&
				cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn chain_spec_file_without_version_works() -> anyhow::Result<()> {
		let chain = "asset-hub-paseo-local";
		let temp_dir = tempdir()?;

		let file = chain_spec_file(chain, None, temp_dir.path()).await?.unwrap();

		// When no version is specified, it should still create the SourcedArchive
		// but the tag will be resolved later
		assert!(matches!(file, SourcedArchive::Source { name, archive_type, .. }
			if name == "asset-hub-paseo-local.json" &&
				archive_type == ArchiveType::File
		));
		Ok(())
	}

	#[tokio::test]
	async fn chain_spec_file_returns_none_when_no_match() -> anyhow::Result<()> {
		let temp_dir = tempdir()?;
		assert_eq!(chain_spec_file("rococo-local", None, temp_dir.path()).await?, None);
		assert_eq!(chain_spec_file("unknown-chain", None, temp_dir.path()).await?, None);
		Ok(())
	}

	#[tokio::test]
	async fn chain_spec_file_returns_none_for_kusama() -> anyhow::Result<()> {
		// Kusama uses Binary (chain-spec-generator), not File
		let temp_dir = tempdir()?;
		assert_eq!(chain_spec_file("kusama-local", None, temp_dir.path()).await?, None);
		Ok(())
	}

	#[tokio::test]
	async fn chain_spec_file_returns_none_for_polkadot() -> anyhow::Result<()> {
		// Polkadot uses Binary (chain-spec-generator), not File
		let temp_dir = tempdir()?;
		assert_eq!(chain_spec_file("polkadot-local", None, temp_dir.path()).await?, None);
		Ok(())
	}

	#[tokio::test]
	async fn chain_spec_file_returns_none_for_westend() -> anyhow::Result<()> {
		// Westend doesn't have File property
		let temp_dir = tempdir()?;
		assert_eq!(chain_spec_file("westend-local", None, temp_dir.path()).await?, None);
		Ok(())
	}

	#[test]
	fn from_chain_matches_parachain_chains() {
		// Verify that parachain chain names match to the correct relay runtime
		for (parachain_chain, expected_relay) in [
			("asset-hub-paseo-local", Paseo),
			("bridge-hub-paseo-local", Paseo),
			("coretime-paseo-local", Paseo),
			("people-paseo-local", Paseo),
			("passet-hub-paseo-local", Paseo),
		] {
			assert_eq!(
				Runtime::from_chain(parachain_chain).unwrap(),
				expected_relay,
				"Chain '{}' should match to {:?}",
				parachain_chain,
				expected_relay
			);
		}
	}
}
