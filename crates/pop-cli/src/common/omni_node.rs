use crate::{
	cli::traits::Cli,
	common::binary::{check_and_prompt, BinaryGenerator},
	impl_binary_generator,
};
use pop_common::{
	git::GitHub,
	polkadot_sdk::sort_by_latest_semantic_version,
	sourcing::{
		filters::prefix,
		traits::{
			enums::{Source as _, *},
			Source as SourceT,
		},
		ArchiveFileSpec, Binary,
		GitHub::*,
		Source,
	},
	target, Error,
};
use std::path::{Path, PathBuf};
use strum_macros::EnumProperty;

#[derive(Debug, EnumProperty, PartialEq)]
pub(super) enum PolkadotOmniNodeCli {
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot",
		Binary = "polkadot-omni-node",
		Fallback = "v0.9.0"
	))]
	PolkadotOmniNode,
}

impl SourceT for PolkadotOmniNodeCli {
	type Error = Error;
	/// Defines the source of the binary required for testing runtime upgrades.
	fn source(&self) -> Result<Source, Error> {
		// Source from GitHub release asset
		let repo = GitHub::parse(self.repository())?;
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
			contents: vec![ArchiveFileSpec::new(binary.into(), Some(binary.into()), true)],
			latest: None,
		}))
	}
}

impl_binary_generator!(PolkadotOmniNodeGenerator, polkadot_omni_node_generator);

/// Generate the source of the `polkadot-omni-node` binary on the remote repository.
///
/// # Arguments
/// * `cache` - The path to the directory where the binary should be cached.
/// * `version` - An optional version string. If `None`, the latest available version is used.
pub async fn polkadot_omni_node_generator(
	cache: PathBuf,
	version: Option<&str>,
) -> Result<Binary, Error> {
	let cli = PolkadotOmniNodeCli::PolkadotOmniNode;
	let name = cli.binary().to_string();
	let source = cli
		.source()?
		.resolve(&name, version, cache.as_path(), |f| prefix(f, &name))
		.await
		.into();
	let binary = Binary::Source { name, source, cache: cache.to_path_buf() };
	Ok(binary)
}

/// Sources and manages the polkadot-omni-node binary, handling download and installation if needed.
///
/// # Arguments
///
/// * `cli` - Mutable reference implementing the Cli trait for user interaction
/// * `cache_path` - Path where the binary should be cached
/// * `skip_confirm` - Whether to skip confirmation prompts during installation
///
/// # Returns
///
/// * `anyhow::Result<PathBuf>` - Path to the installed binary on success, or an error
pub async fn source_polkadot_omni_node_binary(
	cli: &mut impl Cli,
	cache_path: &Path,
	skip_confirm: bool,
) -> anyhow::Result<PathBuf> {
	check_and_prompt::<PolkadotOmniNodeGenerator>(
		cli,
		"polkadot-omni-node",
		cache_path,
		skip_confirm,
	)
	.await
}
