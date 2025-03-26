// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use contract_extrinsics::{ExtrinsicOpts, MapAccountCommandBuilder, MapAccountExec};
use ink_env::DefaultEnvironment;
use pop_common::{DefaultConfig, Keypair};
use sp_core::H160;

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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		contracts_node_generator, mock_build_process, new_environment, run_contracts_node,
	};
	use anyhow::Result;
	use contract_extrinsics::ExtrinsicOptsBuilder;
	use pop_common::{find_free_port, set_executable_permission};
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

		let signer = dev::alice();
		let extrinsic_opts: ExtrinsicOpts<DefaultConfig, DefaultEnvironment, Keypair> =
			ExtrinsicOptsBuilder::new(signer)
				.file(Some(current_dir.join("./tests/files/testing.contract")))
				.url(Url::parse(&localhost_url)?)
				.done();
		let map = AccountMapper::new(&extrinsic_opts).await?;
		assert!(map.needs_mapping().await?);

		let address = map.map_account().await?;
		assert_eq!(address.to_string(), "0x9621dde636de098b43efb0fa9b61facfe328f99d");

		assert!(!map.needs_mapping().await?);

		// Stop the process contracts-node after test.
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;
		Ok(())
	}
}
