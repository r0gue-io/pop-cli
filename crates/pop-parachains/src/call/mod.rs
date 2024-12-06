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
pub async fn set_up_client(url: &str) -> Result<OnlineClient<SubstrateConfig>, Error> {
	OnlineClient::<SubstrateConfig>::from_url(url)
		.await
		.map_err(|e| Error::ConnectionFailure(e.to_string()))
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
/// * `client` - The client used to interact with the chain.
/// * `tx` - The transaction to be signed and submitted.
/// * `suri` - The secret URI (e.g., mnemonic or private key) for signing the extrinsic.
pub async fn sign_and_submit_extrinsic(
	client: OnlineClient<SubstrateConfig>,
	tx: DynamicPayload,
	suri: &str,
) -> Result<String, Error> {
	let signer = create_signer(suri)?;
	let result = client
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
/// * `client` - The client used to interact with the chain.
/// * `tx` - The transaction whose call data will be encoded and returned.
pub fn encode_call_data(
	client: &OnlineClient<SubstrateConfig>,
	tx: &DynamicPayload,
) -> Result<String, Error> {
	let call_data = tx
		.encode_call_data(&client.metadata())
		.map_err(|e| Error::CallDataEncodingError(e.to_string()))?;
	Ok(format!("0x{}", hex::encode(call_data)))
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::set_up_client;
	use anyhow::Result;

	#[tokio::test]
	async fn set_up_client_works() -> Result<()> {
		assert!(matches!(
			set_up_client("wss://wronguri.xyz").await,
			Err(Error::ConnectionFailure(_))
		));
		set_up_client("wss://rpc1.paseo.popnetwork.xyz").await?;
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
		let client = set_up_client("wss://rpc1.paseo.popnetwork.xyz").await?;
		let extrinsic = construct_extrinsic("System", "remark", vec!["0x11".to_string()]).await?;
		assert_eq!(encode_call_data(&client, &extrinsic)?, "0x00000411");
		Ok(())
	}

	#[tokio::test]
	async fn sign_and_submit_wrong_extrinsic_fails() -> Result<()> {
		let client = set_up_client("wss://rpc1.paseo.popnetwork.xyz").await?;
		let tx =
			construct_extrinsic("WrongPallet", "wrongExtrinsic", vec!["0x11".to_string()]).await?;
		assert!(matches!(
			sign_and_submit_extrinsic(client, tx, "//Alice").await,
			Err(Error::ExtrinsicSubmissionError(message)) if message.contains("PalletNameNotFound(\"WrongPallet\"))")
		));
		Ok(())
	}
}
