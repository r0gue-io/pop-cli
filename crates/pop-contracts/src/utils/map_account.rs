// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, DefaultEnvironment};
use contract_extrinsics_inkv6::{ExtrinsicOpts, MapAccountCommandBuilder, MapAccountExec};
use subxt_inkv6::{ext::scale_encode::EncodeAsType, utils::H160, PolkadotConfig as DefaultConfig};
use subxt_signer_inkv6::{sr25519::Keypair, SecretUri};

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
#[encode_as_type(crate_path = "subxt_inkv6::ext::scale_encode")]
pub(crate) struct MapAccount {}

impl MapAccount {
	// Construct an empty `MapAccount` payload.
	pub(crate) fn new() -> Self {
		Self {}
	}
	// Create a call to `Revive::map_account` with no arguments.
	pub(crate) fn build(self) -> subxt_inkv6::tx::DefaultPayload<Self> {
		subxt_inkv6::tx::DefaultPayload::new("Revive", "map_account", self)
	}
}

/// TODO: Duplicated function with the one in pop-common, temporary function due to dependency
/// issues. Create a keypair from a secret URI.
///
/// # Arguments
/// `suri` - Secret URI string used to generate the `Keypair`.
pub fn create_signer(suri: &str) -> Result<Keypair, Error> {
	let uri = <SecretUri as std::str::FromStr>::from_str(suri)
		.map_err(|e| Error::ParseSecretURI(format!("{}", e)))?;
	let keypair = Keypair::from_uri(&uri).map_err(|e| Error::KeyPairCreation(format!("{}", e)))?;
	Ok(keypair)
}