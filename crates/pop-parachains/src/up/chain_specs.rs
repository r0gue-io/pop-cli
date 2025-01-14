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
use std::path::Path;
use strum::{EnumProperty as _, VariantArray as _};
use strum_macros::{AsRefStr, EnumProperty, VariantArray};

/// A supported runtime.
#[derive(AsRefStr, Debug, EnumProperty, PartialEq, VariantArray)]
pub(super) enum Runtime {
	/// Kusama.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot-runtimes",
		Binary = "chain-spec-generator",
		Chain = "kusama-local",
		Fallback = "v1.3.3"
	))]
	Kusama,
	/// Paseo.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/paseo-runtimes",
		Binary = "chain-spec-generator",
		Chain = "paseo-local",
		Fallback = "v1.3.4"
	))]
	Paseo,
	/// Polkadot.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot-runtimes",
		Binary = "chain-spec-generator",
		Chain = "polkadot-local",
		Fallback = "v1.3.3"
	))]
	Polkadot,
}

impl TryInto for &Runtime {
	/// Attempt the conversion.
	///
	/// # Arguments
	/// * `tag` - If applicable, a tag used to determine a specific release.
	/// * `latest` - If applicable, some specifier used to determine the latest source.
	fn try_into(&self, tag: Option<String>, latest: Option<String>) -> Result<Source, Error> {
		// Source from GitHub release asset
		let repo = GitHub::parse(self.repository())?;
		let name = self.name().to_lowercase();
		let binary = self.binary();
		Ok(Source::GitHub(ReleaseArchive {
			owner: repo.org,
			repository: repo.name,
			tag,
			tag_format: self.tag_format().map(|t| t.into()),
			archive: format!("{binary}-{}.tar.gz", target()?),
			contents: vec![(binary, Some(format!("{name}-{binary}")))],
			latest,
		}))
	}
}

impl Runtime {
	/// The chain spec identifier.
	fn chain(&self) -> &'static str {
		self.get_str("Chain").expect("expected specification of `Chain`")
	}

	/// The name of the runtime.
	fn name(&self) -> &str {
		self.as_ref()
	}
}

impl pop_common::sourcing::traits::Source for Runtime {}

pub(super) async fn chain_spec_generator(
	chain: &str,
	version: Option<&str>,
	cache: &Path,
) -> Result<Option<Binary>, Error> {
	if let Some(runtime) =
		Runtime::VARIANTS.iter().find(|r| chain.to_lowercase().ends_with(r.chain()))
	{
		let name = format!("{}-{}", runtime.name().to_lowercase(), runtime.binary());
		let releases = runtime.releases().await?;
		let tag = Binary::resolve_version(&name, version, &releases, cache);
		// Only set latest when caller has not explicitly specified a version to use
		let latest = version.is_none().then(|| releases.first().map(|v| v.to_string())).flatten();
		let binary = Binary::Source {
			name: name.to_string(),
			source: TryInto::try_into(&runtime, tag, latest)?,
			cache: cache.to_path_buf(),
		};
		return Ok(Some(binary));
	}
	Ok(None)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::tempdir;

	#[tokio::test]
	async fn kusama_works() -> anyhow::Result<()> {
		let expected = Runtime::Kusama;
		let version = "v1.3.3";
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
					tag_format: None,
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: ["chain-spec-generator"].map(|b| (b, Some(format!("kusama-{b}").to_string()))).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}) &&
				cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn paseo_works() -> anyhow::Result<()> {
		let expected = Runtime::Paseo;
		let version = "v1.3.4";
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
					tag_format: None,
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: ["chain-spec-generator"].map(|b| (b, Some(format!("paseo-{b}").to_string()))).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}) &&
				cache == temp_dir.path()
		));
		Ok(())
	}

	#[tokio::test]
	async fn polkadot_works() -> anyhow::Result<()> {
		let expected = Runtime::Polkadot;
		let version = "v1.3.3";
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
					tag_format: None,
					archive: format!("chain-spec-generator-{}.tar.gz", target()?),
					contents: ["chain-spec-generator"].map(|b| (b, Some(format!("polkadot-{b}").to_string()))).to_vec(),
					latest: binary.latest().map(|l| l.to_string()),
				}) &&
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
}
