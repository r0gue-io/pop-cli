// SPDX-License-Identifier: GPL-3.0

use super::{traits::Requires, *};
use crate::{
	accounts,
	traits::{Args, Binary},
};
use pop_common::{
	polkadot_sdk::sort_by_latest_semantic_version,
	sourcing::{traits::Source as SourceT, GitHub::ReleaseArchive, Source},
	target,
};
use serde_json::{json, Map, Value};
use sp_core::crypto::Ss58Codec;
use std::collections::HashMap;

/// Pop Network makes it easy for smart contract developers to use the power of Polkadot.
#[derive(Clone)]
pub struct Pop(Rollup);
impl Pop {
	/// A new instance of Pop.
	///
	/// # Arguments
	/// * `id` - The rollup identifier.
	/// * `chain` - The identifier of the chain, as used by the chain specification.
	pub fn new(id: Id, chain: impl Into<String>) -> Self {
		Self(Rollup::new("pop", id, chain))
	}
}

impl SourceT for Pop {
	/// Defines the source of a binary.
	fn source(&self) -> Result<Source, pop_common::Error> {
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
			contents: vec![(binary, None, true)],
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
	/// Defines the requirements of a rollup, namely which other chains it depends on and any
	/// corresponding overrides to genesis state.
	fn requires(&self) -> Option<HashMap<RollupTypeId, Override>> {
		let id = self.id();
		let amount: u128 = 1_200_000_000_000_000_000;

		Some(HashMap::from([(
			RollupTypeId::of::<AssetHub>(),
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

impl_rollup!(Pop);
