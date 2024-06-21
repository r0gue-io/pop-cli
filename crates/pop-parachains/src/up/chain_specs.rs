// SPDX-License-Identifier: GPL-3.0
use crate::{
	up::{
		sourcing::{
			self,
			traits::{Source as _, *},
			GitHub::*,
			Source,
		},
		target,
	},
	Binary, Error,
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
		Fallback = "v1.2.7"
	))]
	Kusama,
	/// Paseo.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/paseo-runtimes",
		Binary = "chain-spec-generator",
		Chain = "paseo-local",
		Fallback = "v1.2.4"
	))]
	Paseo,
	/// Polkadot.
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot-runtimes",
		Binary = "chain-spec-generator",
		Chain = "polkadot-local",
		Fallback = "v1.2.7"
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
		Ok(match self {
			_ => {
				// Source from GitHub release asset
				let repo = crate::GitHub::parse(self.repository())?;
				let name = self.name().to_lowercase();
				let binary = self.binary();
				Source::GitHub(ReleaseArchive {
					owner: repo.org,
					repository: repo.name,
					tag,
					tag_format: self.tag_format().map(|t| t.into()),
					archive: format!("{binary}-{}.tar.gz", target()?),
					contents: vec![(binary, Some(format!("{name}-{binary}")))],
					latest,
				})
			},
		})
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

impl sourcing::traits::Source for Runtime {}

pub(super) async fn chain_spec_generator(
	chain: &str,
	version: Option<&str>,
	cache: &Path,
) -> Result<Option<Binary>, Error> {
	for runtime in Runtime::VARIANTS.iter().filter(|r| chain.to_lowercase().ends_with(r.chain())) {
		let name = format!("{}-{}", runtime.name().to_lowercase(), runtime.binary());
		let releases = runtime.releases().await?;
		let tag = Binary::resolve_version(&name, version, &releases, cache);
		// Only set latest when caller has not explicitly specified a version to use
		let latest = version
			.is_none()
			.then(|| releases.iter().nth(0).map(|v| v.to_string()))
			.flatten();
		let binary = Binary::Source {
			name: name.to_string(),
			source: TryInto::try_into(&runtime, tag, latest)?,
			cache: cache.to_path_buf(),
		};
		return Ok(Some(binary));
	}
	Ok(None)
}
