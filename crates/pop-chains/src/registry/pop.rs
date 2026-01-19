// SPDX-License-Identifier: GPL-3.0

use super::{traits::Requires, *};
use crate::{
	Error, accounts,
	traits::{Args, Binary},
};
use pop_common::{
	polkadot_sdk::sort_by_latest_semantic_version,
	sourcing::{ArchiveFileSpec, GitHub::ReleaseArchive, Source, traits::Source as SourceT},
	target,
};
use serde_json::{Map, Value, json};
use sp_core::crypto::Ss58Codec;
use std::collections::HashMap;

/// Pop Network makes it easy for smart contract developers to use the power of Polkadot.
#[derive(Clone)]
pub struct Pop(Chain);
impl Pop {
	/// A new instance of Pop.
	///
	/// # Arguments
	/// * `id` - The chain identifier.
	/// * `chain` - The identifier of the chain, as used by the chain specification.
	pub fn new(id: Id, chain: impl Into<String>) -> Self {
		Self(Chain::new("pop", id, chain))
	}
}

impl SourceT for Pop {
	type Error = Error;
	/// Defines the source of a binary.
	fn source(&self) -> Result<Source, Error> {
		// Source from GitHub release asset
		let binary = self.binary();
		Ok(Source::GitHub(ReleaseArchive {
			owner: "r0gue-io".into(),
			repository: "pop-node".into(),
			tag: None,
			tag_pattern: Some("node-{version}".into()),
			prerelease: false,
			version_comparator: sort_by_latest_semantic_version,
			fallback: "v0.3.0".into(),
			archive: format!("{binary}-{}.tar.gz", target()?),
			contents: vec![ArchiveFileSpec::new(binary.into(), None, true)],
			latest: None,
		}))
	}
}

impl Binary for Pop {
	fn binary(&self) -> &'static str {
		"pop-node"
	}
}

impl Requires for Pop {
	/// Defines the requirements of a chain, namely which other chains it depends on and any
	/// corresponding overrides to genesis state.
	fn requires(&self) -> Option<HashMap<ChainTypeId, Override>> {
		let id = self.id();
		let amount: u128 = 1_200_000_000_000_000_000;

		Some(HashMap::from([(
			ChainTypeId::of::<AssetHub>(),
			Box::new(move |genesis_overrides: &mut Map<String, Value>| {
				let sovereign_account = accounts::sibl(id).to_ss58check();
				let endowment = json!([sovereign_account, amount]);
				// Add sovereign account endowment
				genesis_overrides
					.entry("balances")
					.and_modify(|balances| {
						let balances =
							balances.as_object_mut().expect("expected balances as object");
						match balances.get_mut("balances") {
							None => {
								balances.insert("balances".to_string(), json!([endowment]));
							},
							Some(balances) => {
								let balances =
									balances.as_array_mut().expect("expected balances as array");
								balances.push(endowment.clone());
							},
						}
					})
					.or_insert(json!({"balances": [endowment]}));
			}) as Override,
		)]))
	}
}

impl Args for Pop {
	fn args(&self) -> Option<Vec<&str>> {
		Some(vec![
			"-lpop-api::extension=debug",
			"-lruntime::contracts=trace",
			"-lruntime::revive=trace",
			"-lruntime::revive::strace=trace",
			"-lxcm=trace",
			"--enable-offchain-indexing=true",
		])
	}
}

impl GenesisOverrides for Pop {}

impl_chain!(Pop);

#[cfg(test)]
mod tests {
	use super::*;
	use pop_common::SortedSlice;
	use std::ptr::fn_addr_eq;

	#[test]
	fn source_works() {
		let pop = Pop::new(3395, "pop");
		assert!(matches!(
			pop.source().unwrap(),
			Source::GitHub(ReleaseArchive { owner, repository, tag, tag_pattern, prerelease, version_comparator, fallback, archive, contents, latest })
				if owner == "r0gue-io" &&
					repository == "pop-node" &&
					tag.is_none() &&
					tag_pattern == Some("node-{version}".into()) &&
					!prerelease &&
					fn_addr_eq(version_comparator, sort_by_latest_semantic_version as for<'a> fn(&'a mut [String]) -> SortedSlice<'a, String>) &&
					fallback == "v0.3.0" &&
					archive == format!("pop-node-{}.tar.gz", target().unwrap()) &&
					contents == vec![ArchiveFileSpec::new("pop-node".into(), None, true)] &&
					latest.is_none()
		));
	}

	#[test]
	fn binary_works() {
		let pop = Pop::new(3395, "pop");
		assert_eq!(pop.binary(), "pop-node");
	}

	#[test]
	fn requires_asset_hub() {
		let pop = Pop::new(3395, "pop");
		let mut requires = pop.requires().unwrap();
		let r#override = requires.get_mut(&ChainTypeId::of::<AssetHub>()).unwrap();
		let expected = json!({
			"balances": {
				"balances": [["5Eg2fnsomjubNiqxnqSSeVwcmQYQzsHdyr79YhcJDKRYfPCL", 1200000000000000000u64]]
		}});
		// Empty
		let mut overrides = Map::new();
		r#override(&mut overrides);
		assert_eq!(Value::Object(overrides), expected);
		// Inserts
		let mut overrides = Map::new();
		overrides.insert("balances".to_string(), json!({}));
		r#override(&mut overrides);
		assert_eq!(Value::Object(overrides), expected);
		// Updates
		let mut overrides = Map::new();
		overrides.insert("balances".to_string(), json!({ "balances": []}));
		r#override(&mut overrides);
		assert_eq!(Value::Object(overrides), expected);
	}

	#[test]
	fn args_works() {
		let pop = Pop::new(3395, "pop");
		assert_eq!(
			pop.args().unwrap(),
			vec![
				"-lpop-api::extension=debug",
				"-lruntime::contracts=trace",
				"-lruntime::revive=trace",
				"-lruntime::revive::strace=trace",
				"-lxcm=trace",
				"--enable-offchain-indexing=true",
			]
		);
	}
}
