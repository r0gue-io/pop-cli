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
