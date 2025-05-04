// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, DefaultEnvironment};
use contract_extrinsics_inkv6::{ExtrinsicOpts, MapAccountCommandBuilder, MapAccountExec};
use pop_common::{DefaultConfig, Keypair};
use subxt::{ext::scale_encode::EncodeAsType, utils::H160};

/// A helper struct for performing account mapping operations.
pub struct AccountMapper {
	map_exec: MapAccountExec<DefaultConfig, DefaultEnvironment, Keypair>,
}

impl AccountMapper {
	/// Creates a new `AccountMapper` instance.
	///
	/// # Arguments
	/// * `extrinsic_opts` - Options used to build and submit a contract extrinsic.
	pub async fn new(
		extrinsic_opts: &ExtrinsicOpts<DefaultConfig, DefaultEnvironment, Keypair>,
	) -> Result<Self, Error> {
		let map_exec = MapAccountCommandBuilder::new(extrinsic_opts.clone()).done().await?;
		Ok(Self { map_exec })
	}

	/// Checks whether the account needs to be mapped by performing a dry run.
	pub async fn needs_mapping(&self) -> Result<bool, Error> {
		Ok(self.map_exec.map_account_dry_run().await.is_ok())
	}

	/// Performs the actual account mapping.
	pub async fn map_account(&self) -> Result<H160, Error> {
		let result = self
			.map_exec
			.map_account()
			.await
			.map_err(|e| Error::MapAccountError(e.to_string()))?;
		Ok(result.address)
	}
}

// Create a call to `Revive::map_account`.
#[derive(Debug, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub(crate) struct MapAccount {}

impl MapAccount {
	// Construct an empty `MapAccount` payload.
	pub(crate) fn new() -> Self {
		Self {}
	}
	// Create a call to `Revive::map_account` with no arguments.
	pub(crate) fn build(self) -> subxt::tx::DefaultPayload<Self> {
		subxt::tx::DefaultPayload::new("Revive", "map_account", self)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		contracts_node_generator, mock_build_process, new_environment, run_contracts_node,
	};
	use anyhow::Result;
	use contract_extrinsics_inkv6::ExtrinsicOptsBuilder;
	use pop_common::{find_free_port, parse_h160_account, set_executable_permission};
	use std::{env, process::Command};
	use subxt_signer::sr25519::dev;
	use url::Url;

	#[tokio::test]
	async fn map_account_works() -> Result<()> {
		let random_port = find_free_port(None);
		let localhost_url = format!("ws://127.0.0.1:{}", random_port);
		let temp_dir = new_environment("testing")?;
		let current_dir = env::current_dir().expect("Failed to get current directory");
		mock_build_process(
			temp_dir.path().join("testing"),
			current_dir.join("./tests/files/testing.contract"),
			current_dir.join("./tests/files/testing.json"),
		)?;
		// Run the process contracts-node for the test.
		let cache = temp_dir.path().join("");
		let binary = contracts_node_generator(cache, None).await?;
		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;
		let process = run_contracts_node(binary.path(), None, random_port).await?;
		// Alice is mapped when running the contracts-node.
		let signer = dev::bob();
		let extrinsic_opts: ExtrinsicOpts<DefaultConfig, DefaultEnvironment, Keypair> =
			ExtrinsicOptsBuilder::new(signer)
				.file(Some(current_dir.join("./tests/files/testing.contract")))
				.url(Url::parse(&localhost_url)?)
				.done();
		let map = AccountMapper::new(&extrinsic_opts).await?;
		assert!(map.needs_mapping().await?);

		let address = map.map_account().await?;
		assert_eq!(address, parse_h160_account("0x41dccbd49b26c50d34355ed86ff0fa9e489d1e01")?);

		assert!(!map.needs_mapping().await?);

		// Stop the process contracts-node after test.
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;
		Ok(())
	}
}
