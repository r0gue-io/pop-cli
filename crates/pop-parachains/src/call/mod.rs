// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::create_signer;
use subxt::{
	dynamic::Value,
	tx::{DynamicPayload, Payload},
	OnlineClient, SubstrateConfig,
};

pub mod metadata;

/// Sets up an OnlineClient instance for connecting to a blockchain.
///
/// # Arguments
/// * `url` - Endpoint of the node.
pub async fn set_up_api(url: &str) -> Result<OnlineClient<SubstrateConfig>, Error> {
	let api = OnlineClient::<SubstrateConfig>::from_url(url)
		.await
		.map_err(|e| Error::ApiConnectionFailure(e.to_string()))?;
	Ok(api)
}

/// Constructs a dynamic extrinsic payload for a specified pallet and extrinsic.
///
/// # Arguments
/// * `pallet_name` - The name of the pallet containing the extrinsic.
/// * `extrinsic_name` - The specific extrinsic name within the pallet.
/// * `args` - A vector of string arguments to be passed to the extrinsic.
pub async fn construct_extrinsic(
	pallet_name: &str,
	extrinsic_name: &str,
	args: Vec<String>,
) -> Result<DynamicPayload, Error> {
	let parsed_args: Vec<Value> = metadata::parse_extrinsic_arguments(args).await?;
	Ok(subxt::dynamic::tx(pallet_name, extrinsic_name, parsed_args))
}

/// Signs and submits a given extrinsic to the blockchain.
///
/// # Arguments
/// * `api` - Reference to an `OnlineClient` connected to the chain.
/// * `tx` - The transaction to be signed and submitted.
/// * `suri` - The secret URI (e.g., mnemonic or private key) for signing the extrinsic.
pub async fn sign_and_submit_extrinsic(
	api: OnlineClient<SubstrateConfig>,
	tx: DynamicPayload,
	suri: &str,
) -> Result<String, Error> {
	let signer = create_signer(suri)?;
	let result = api
		.tx()
		.sign_and_submit_then_watch_default(&tx, &signer)
		.await
		.map_err(|e| Error::ExtrinsicSubmissionError(format!("{:?}", e)))?
		.wait_for_finalized_success()
		.await
		.map_err(|e| Error::ExtrinsicSubmissionError(format!("{:?}", e)))?;
	Ok(format!("{:?}", result.extrinsic_hash()))
}

/// Encodes the call data for a given extrinsic into a hexadecimal string.
///
/// # Arguments
/// * `api` - Reference to an `OnlineClient` connected to the chain.
/// * `tx` - The transaction whose call data will be encoded and returned.
pub fn encode_call_data(
	api: &OnlineClient<SubstrateConfig>,
	tx: &DynamicPayload,
) -> Result<String, Error> {
	let call_data = tx
		.encode_call_data(&api.metadata())
		.map_err(|e| Error::CallDataEncodingError(e.to_string()))?;
	Ok(format!("0x{}", hex::encode(call_data)))
}

/// This struct implements the [`Payload`] trait and is used to submit
/// pre-encoded SCALE call data directly, without the dynamic construction of transactions.
struct CallData(Vec<u8>);

impl Payload for CallData {
	fn encode_call_data_to(
		&self,
		_: &subxt::Metadata,
		out: &mut Vec<u8>,
	) -> Result<(), subxt::ext::subxt_core::Error> {
		out.extend_from_slice(&self.0);
		Ok(())
	}
}

/// Signs and submits a given extrinsic to the blockchain.
///
/// # Arguments
/// * `api` - Reference to an `OnlineClient` connected to the chain.
/// * `call_data` -SCALE encoded bytes representing the call data of the transaction.
/// * `suri` - The secret URI (e.g., mnemonic or private key) for signing the extrinsic.
pub async fn sign_and_submit_extrinsic_with_call_data(
	api: OnlineClient<SubstrateConfig>,
	call_data: &str,
	suri: &str,
) -> Result<String, Error> {
	let signer = create_signer(suri)?;
	let call_data_bytes = hex::decode(call_data.trim_start_matches("0x"))
		.map_err(|e| Error::CallDataDecodingError(e.to_string()))?;
	let payload = CallData(call_data_bytes);
	let result = api
		.tx()
		.sign_and_submit_then_watch_default(&payload, &signer)
		.await
		.map_err(|e| Error::ExtrinsicSubmissionError(format!("{:?}", e)))?
		.wait_for_finalized_success()
		.await
		.map_err(|e| Error::ExtrinsicSubmissionError(format!("{:?}", e)))?;
	Ok(format!("{:?}", result.extrinsic_hash()))
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::set_up_api;
	use anyhow::Result;

	#[tokio::test]
	async fn set_up_api_works() -> Result<()> {
		assert!(matches!(
			set_up_api("wss://wronguri.xyz").await,
			Err(Error::ApiConnectionFailure(_))
		));
		set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		Ok(())
	}

	#[tokio::test]
	async fn construct_extrinsic_works() -> Result<()> {
		// Wrong parameters
		assert!(matches!(
			construct_extrinsic(
				"Balances",
				"transfer_allow_death",
				vec!["Bob".to_string(), "100".to_string()],
			)
			.await,
			Err(Error::ParamProcessingError)
		));
		// Valid parameters
		let extrinsic = construct_extrinsic(
			"Balances",
			"transfer_allow_death",
			vec![
				"Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string(),
				"100".to_string(),
			],
		)
		.await?;
		assert_eq!(extrinsic.call_name(), "transfer_allow_death");
		assert_eq!(extrinsic.pallet_name(), "Balances");
		Ok(())
	}

	#[tokio::test]
	async fn encode_call_data_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let extrinsic = construct_extrinsic("System", "remark", vec!["0x11".to_string()]).await?;
		assert_eq!(encode_call_data(&api, &extrinsic)?, "0x00000411");
		Ok(())
	}
}
