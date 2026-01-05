// SPDX-License-Identifier: GPL-3.0

use crate::{Error, registry::traits};
use pop_common::{
	git::GitHub,
	polkadot_sdk::sort_by_latest_semantic_version,
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
use std::path::Path;
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
		Repository = "https://github.com/r0gue-io/paseo-runtimes",
		Binary = "chain-spec-generator",
		Chain = "paseo-local",
		Fallback = "v1.4.1"
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
			contents: vec![ArchiveFileSpec::new(
				binary.into(),
				Some(format!("{name}-{binary}").into()),
				true,
			)],
			latest: None,
		}))
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

	/// Returns the chains registered for the relay chain.
	pub fn chains(&self) -> &'static [Box<dyn traits::Chain>] {
		crate::registry::chains(self)
	}
}

pub(super) async fn chain_spec_generator(
	chain: &str,
	version: Option<&str>,
	cache: &Path,
) -> Result<Option<Binary>, Error> {
	if let Some(runtime) = Runtime::from_chain(chain) {
		if runtime == Runtime::Westend {
			// Westend runtimes included with binary.
			return Ok(None);
		}
		let name = format!("{}-{}", runtime.name().to_lowercase(), runtime.binary());
		let source = runtime
			.source()?
			.resolve(&name, version, cache, |f| prefix(f, &name))
			.await
			.into();
		let binary = Binary::Source { name, source, cache: cache.to_path_buf() };
		return Ok(Some(binary));
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
		let version = "v1.4.1";
		let temp_dir = tempdir()?;
		let binary = chain_spec_generator("kusama-local", Some(version), temp_dir.path())
			.await?
			.unwrap();
		assert!(matches!(binary, Binary::Source { name, source, cache }
			if name == format!("{}-{}", expected.name().to_lowercase(), expected.binary()) &&
				source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "polkadot-runtimes".to_string(),
					tag: Some(version.to_string()),
					tag_pattern: None,
					prerelease: false,
					version_comparator: sort_by_latest_semantic_version,
					fallback: expected.fallback().to_string(),
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: ["chain-spec-generator"].map(|b| ArchiveFileSpec::new(b.into(), Some(format!("kusama-{b}").into()), true)).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}).into() &&
				cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn paseo_works() -> anyhow::Result<()> {
		let expected = Runtime::Paseo;
		let version = "v1.4.1";
		let temp_dir = tempdir()?;
		let binary = chain_spec_generator("paseo-local", Some(version), temp_dir.path())
			.await?
			.unwrap();
		assert!(matches!(binary, Binary::Source { name, source, cache }
			if name == format!("{}-{}", expected.name().to_lowercase(), expected.binary()) &&
				source == Source::GitHub(ReleaseArchive {
					owner: "r0gue-io".to_string(),
					repository: "paseo-runtimes".to_string(),
					tag: Some(version.to_string()),
					tag_pattern: None,
					prerelease: false,
					version_comparator: sort_by_latest_semantic_version,
					fallback: expected.fallback().to_string(),
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: ["chain-spec-generator"].map(|b| ArchiveFileSpec::new(b.into(), Some(format!("paseo-{b}").into()), true)).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}).into() &&
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
		assert!(matches!(binary, Binary::Source { name, source, cache }
			if name == format!("{}-{}", expected.name().to_lowercase(), expected.binary()) &&
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
				cache == temp_dir.path()
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
	fn chains_works() {
		let comparator = |chains: &[Box<dyn traits::Chain>]| {
			chains
				.iter()
				.map(|r| (r.id(), r.chain().to_string(), r.name().to_string()))
				.collect::<Vec<_>>()
		};
		{};
		for runtime in Runtime::VARIANTS {
			assert_eq!(comparator(runtime.chains()), comparator(crate::registry::chains(runtime)));
		}
	}
}
