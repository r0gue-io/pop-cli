// SPDX-License-Identifier: GPL-3.0

use crate::{DefaultEnvironment, errors::Error};
use anyhow::anyhow;
use contract_extrinsics::{AccountIdMapper, ExtrinsicOpts};
use pop_common::{DefaultConfig, Keypair};
use scale::Encode;
use subxt::{
	OnlineClient, backend,
	backend::{
		legacy::{LegacyRpcMethods, rpc_methods::DryRunResult},
		rpc::RpcClient,
	},
	config::DefaultExtrinsicParamsBuilder,
	ext::{scale_encode::EncodeAsType, subxt_rpcs::methods::legacy::DryRunDecodeError},
	utils::H160,
};

/// A helper struct for performing account mapping operations.
pub struct AccountMapper {
	extrinsic_opts: ExtrinsicOpts<DefaultConfig, DefaultEnvironment, Keypair>,
	client: OnlineClient<DefaultConfig>,
	rpc: LegacyRpcMethods<DefaultConfig>,
}

impl AccountMapper {
	/// Creates a new `AccountMapper` instance.
	///
	/// # Arguments
	/// * `extrinsic_opts` - Options used to build and submit a contract extrinsic.
	pub async fn new(
		extrinsic_opts: &ExtrinsicOpts<DefaultConfig, DefaultEnvironment, Keypair>,
	) -> Result<Self, Error> {
		let rpc_client = RpcClient::from_url(extrinsic_opts.url())
			.await
			.map_err(|e| Error::AnyhowError(e.into()))?;
		let client = OnlineClient::<DefaultConfig>::from_rpc_client(rpc_client.clone())
			.await
			.map_err(|e| Error::AnyhowError(e.into()))?;
		let rpc = backend::legacy::LegacyRpcMethods::new(rpc_client);
		Ok(Self { extrinsic_opts: extrinsic_opts.clone(), client, rpc })
	}

	/// Checks whether the account needs to be mapped by performing a dry run.
	pub async fn needs_mapping(&self) -> Result<bool, Error> {
		Ok(self.dry_run_map_account().await.is_ok())
	}

	/// Performs the actual account mapping.
	pub async fn map_account(&self) -> Result<H160, Error> {
		let call = MapAccount::new().build();
		self.client
			.tx()
			.sign_and_submit_default(&call, self.extrinsic_opts.signer())
			.await
			.map_err(|e| Error::MapAccountError(e.to_string()))?;
		let account_id =
			<Keypair as subxt::tx::Signer<DefaultConfig>>::account_id(self.extrinsic_opts.signer());
		let encoded = account_id.encode();
		Ok(AccountIdMapper::to_address(&encoded))
	}

	async fn dry_run_map_account(&self) -> anyhow::Result<()> {
		let account_id =
			<Keypair as subxt::tx::Signer<DefaultConfig>>::account_id(self.extrinsic_opts.signer());
		let account_nonce = self.get_account_nonce(&account_id).await?;
		let params = DefaultExtrinsicParamsBuilder::new().nonce(account_nonce).build();
		let call = MapAccount::new().build();
		let extrinsic = self
			.client
			.tx()
			.create_partial_offline(&call, params.into())?
			.sign(self.extrinsic_opts.signer());
		let dry_run_result = self.rpc.dry_run(extrinsic.encoded(), None).await?;

		match dry_run_result.into_dry_run_result() {
			Ok(DryRunResult::Success) | Ok(DryRunResult::TransactionValidityError) => Ok(()),
			Ok(DryRunResult::DispatchError(err)) => anyhow::bail!("dispatch error: {err:?}"),
			Err(DryRunDecodeError::WrongNumberOfBytes) => anyhow::bail!(
				"decode error: dry run result was less than 2 bytes, which is invalid"
			),
			Err(DryRunDecodeError::InvalidBytes) =>
				anyhow::bail!("decode error: dry run bytes are not valid"),
		}
	}

	async fn get_account_nonce(
		&self,
		account_id: &<DefaultConfig as subxt::Config>::AccountId,
	) -> anyhow::Result<u64> {
		let best_block = self
			.rpc
			.chain_get_block_hash(None)
			.await?
			.ok_or_else(|| anyhow!("Best block not found"))?;
		let account_nonce =
			self.client.blocks().at(best_block).await?.account_nonce(account_id).await?;
		Ok(account_nonce)
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
