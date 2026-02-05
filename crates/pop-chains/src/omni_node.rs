use pop_common::{
	Error,
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
use std::path::PathBuf;
use strum_macros::EnumProperty;

/// CLI enum for managing Polkadot Omni Node binary sources and configuration.
/// Provides repository information and binary specifications for fetching and managing the node.
#[derive(Debug, EnumProperty, PartialEq)]
pub enum PolkadotOmniNodeCli {
	#[strum(props(
		Repository = "https://github.com/r0gue-io/polkadot",
		Binary = "polkadot-omni-node",
		TagPattern = "polkadot-{version}",
		Fallback = "stable2512"
	))]
	/// Polkadot Omni Node binary. Used to bootstrap parachains without node.
	PolkadotOmniNode,
}

impl SourceT for PolkadotOmniNodeCli {
	type Error = Error;
	/// Creates a Source configuration for fetching the Polkadot Omni Node binary from GitHub.
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

#[cfg(test)]
mod tests {
	use super::*;
	use pop_common::sourcing::TagPattern;
	use strum::EnumProperty;

	#[test]
	fn polkadot_omni_node_cli_properties_work() {
		let cli = PolkadotOmniNodeCli::PolkadotOmniNode;

		// Test enum properties
		assert_eq!(cli.get_str("Repository"), Some("https://github.com/r0gue-io/polkadot"));
		assert_eq!(cli.get_str("Binary"), Some("polkadot-omni-node"));
		assert_eq!(cli.get_str("TagPattern"), Some("polkadot-{version}"));
		assert_eq!(cli.get_str("Fallback"), Some("stable2512"));
	}

	#[test]
	fn polkadot_omni_node_cli_source_works() -> anyhow::Result<()> {
		let cli = PolkadotOmniNodeCli::PolkadotOmniNode;
		let source = cli.source()?;

		// Verify source is GitHub variant
		match source {
			Source::GitHub(ReleaseArchive {
				owner,
				repository,
				tag,
				tag_pattern,
				prerelease,
				fallback,
				archive,
				contents,
				..
			}) => {
				assert_eq!(owner, "r0gue-io");
				assert_eq!(repository, "polkadot");
				assert_eq!(tag, None);
				assert_eq!(tag_pattern, Some(TagPattern::new("polkadot-{version}")));
				assert!(!prerelease);
				assert_eq!(fallback, "stable2512");
				assert!(archive.starts_with("polkadot-omni-node-"));
				assert!(archive.ends_with(".tar.gz"));
				assert_eq!(contents.len(), 1);
				assert_eq!(contents[0].name, "polkadot-omni-node");
				assert!(contents[0].required);
			},
			_ => panic!("Expected GitHub ReleaseArchive source variant"),
		}

		Ok(())
	}

	#[tokio::test]
	async fn polkadot_omni_node_generator_works() -> anyhow::Result<()> {
		let cache = tempfile::tempdir()?;
		let binary = polkadot_omni_node_generator(cache.path().to_path_buf(), None).await?;

		match binary {
			Binary::Source { name, source, cache: cache_path } => {
				assert_eq!(name, "polkadot-omni-node");
				assert_eq!(cache_path, cache.path());
				// Source should be a ResolvedRelease
				match *source {
					Source::GitHub(github) =>
						if let ReleaseArchive { archive, .. } = github {
							assert!(archive.contains("polkadot-omni-node"));
						},
					_ => panic!("Expected GitHub variant"),
				}
			},
			_ => panic!("Expected Binary::Source variant"),
		}

		Ok(())
	}
}
