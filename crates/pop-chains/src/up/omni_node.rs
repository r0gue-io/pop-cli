// SPDX-License-Identifier: GPL-3.0

use crate::Error;
use pop_common::{
	git::GitHub,
	polkadot_sdk::sort_by_latest_stable_version,
	sourcing::{
		traits::{
			enums::{Source as _, *},
			Source as SourceT,
		},
		ArchiveFileSpec,
		GitHub::*,
		Source,
	},
	target,
};
use std::iter::once;
use strum_macros::{EnumProperty, VariantArray};

#[derive(EnumProperty, VariantArray)]
pub(super) enum OmniNode {
	#[strum(props(
		Repository = "https://github.com/Moliholy/r0gue-polkadot",
		Binary = "polkadot-omni-node",
		TagPattern = "polkadot-{version}",
		Fallback = "stable2503-7"
	))]
	PolkadotOmniNode,
	#[strum(props(
		Repository = "https://github.com/Moliholy/r0gue-polkadot",
		Binary = "polkadot-omni-node",
		TagPattern = "polkadot-{version}",
		Fallback = "stable2503-7"
	))]
	ChainSpecBuilder,
}

impl OmniNode {
	pub fn name(&self) -> &str {
		match self {
			OmniNode::PolkadotOmniNode => "polkadot-omni-node",
			OmniNode::ChainSpecBuilder => "chain-spec-builder",
		}
	}
}

impl SourceT for OmniNode {
	type Error = Error;
	fn source(&self) -> Result<Source, Error> {
		let repo = GitHub::parse(self.repository())?;
		Ok(Source::GitHub(ReleaseArchive {
			owner: repo.org,
			repository: repo.name,
			tag: None,
			tag_pattern: self.tag_pattern().map(|t| t.into()),
			prerelease: false,
			version_comparator: sort_by_latest_stable_version,
			fallback: self.fallback().into(),
			archive: format!("{}-{}.tar.gz", self.binary(), target()?),
			contents: once(self.name())
				.map(|n| ArchiveFileSpec::new(n.to_string(), None, true))
				.collect(),
			latest: None,
		}))
	}
}
