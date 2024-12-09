// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, Extrinsic};
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
	extrinsic: &Extrinsic,
	args: Vec<String>,
) -> Result<DynamicPayload, Error> {
	let parsed_args: Vec<Value> =
		metadata::parse_extrinsic_arguments(&extrinsic.params, args).await?;
	Ok(subxt::dynamic::tx(pallet_name, extrinsic.name.clone(), parsed_args))
}

/// Constructs a Sudo extrinsic.
///
/// # Arguments
/// * `tx`: The transaction payload representing the function call to be dispatched with `Root`
///   privileges.
pub async fn construct_sudo_extrinsic(tx: DynamicPayload) -> Result<DynamicPayload, Error> {
	Ok(subxt::dynamic::tx("Sudo", "sudo", [tx.into_value()].to_vec()))
}

/// Signs and submits a given extrinsic.
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

/// Decodes a hex-encoded string into a vector of bytes representing the call data.
///
/// # Arguments
/// * `call_data` - The hex-encoded string representing call data.
pub fn decode_call_data(call_data: &str) -> Result<Vec<u8>, Error> {
	hex::decode(call_data.trim_start_matches("0x"))
		.map_err(|e| Error::CallDataDecodingError(e.to_string()))
}

// This struct implements the [`Payload`] trait and is used to submit
// pre-encoded SCALE call data directly, without the dynamic construction of transactions.
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

/// Signs and submits a given extrinsic.
///
/// # Arguments
/// * `client` - Reference to an `OnlineClient` connected to the chain.
/// * `call_data` - SCALE encoded bytes representing the extrinsic's call data.
/// * `suri` - The secret URI (e.g., mnemonic or private key) for signing the extrinsic.
pub async fn sign_and_submit_extrinsic_with_call_data(
	client: OnlineClient<SubstrateConfig>,
	call_data: Vec<u8>,
	suri: &str,
) -> Result<String, Error> {
	let signer = create_signer(suri)?;
	let payload = CallData(call_data);
	let result = client
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
	use crate::{find_extrinsic_by_name, parse_chain_metadata, set_up_client};
	use anyhow::Result;

	const ALICE_SURI: &str = "//Alice";
	const POP_NETWORK_TESTNET_URL: &str = "wss://rpc1.paseo.popnetwork.xyz";

	#[tokio::test]
	async fn set_up_client_works() -> Result<()> {
		assert!(matches!(
			set_up_client("wss://wronguri.xyz").await,
			Err(Error::ConnectionFailure(_))
		));
		set_up_client(POP_NETWORK_TESTNET_URL).await?;
		Ok(())
	}

	#[tokio::test]
	async fn construct_extrinsic_works() -> Result<()> {
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let pallets = parse_chain_metadata(&client).await?;
		let transfer_allow_death =
			find_extrinsic_by_name(&pallets, "Balances", "transfer_allow_death").await?;

		// Wrong parameters
		assert!(matches!(
			construct_extrinsic(
				"Balances",
				&transfer_allow_death,
				vec![ALICE_SURI.to_string(), "100".to_string()],
			)
			.await,
			Err(Error::ParamProcessingError)
		));
		// Valid parameters
		let extrinsic = construct_extrinsic(
			"Balances",
			&transfer_allow_death,
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
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let pallets = parse_chain_metadata(&client).await?;
		let remark = find_extrinsic_by_name(&pallets, "System", "remark").await?;
		let extrinsic = construct_extrinsic("System", &remark, vec!["0x11".to_string()]).await?;
		assert_eq!(encode_call_data(&client, &extrinsic)?, "0x00000411");
		let extrinsic = construct_extrinsic("System", &remark, vec!["123".to_string()]).await?;
		assert_eq!(encode_call_data(&client, &extrinsic)?, "0x00000c313233");
		let extrinsic = construct_extrinsic("System", &remark, vec!["test".to_string()]).await?;
		assert_eq!(encode_call_data(&client, &extrinsic)?, "0x00001074657374");
		Ok(())
	}

	#[tokio::test]
	async fn decode_call_data_works() -> Result<()> {
		assert!(matches!(decode_call_data("wrongcalldata"), Err(Error::CallDataDecodingError(..))));
		let client = set_up_client("wss://rpc1.paseo.popnetwork.xyz").await?;
		let pallets = parse_chain_metadata(&client).await?;
		let remark = find_extrinsic_by_name(&pallets, "System", "remark").await?;
		let extrinsic = construct_extrinsic("System", &remark, vec!["0x11".to_string()]).await?;
		let expected_call_data = extrinsic.encode_call_data(&client.metadata())?;
		assert_eq!(decode_call_data("0x00000411")?, expected_call_data);
		Ok(())
	}

	#[tokio::test]
	async fn sign_and_submit_wrong_extrinsic_fails() -> Result<()> {
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let extrinsic = Extrinsic {
			name: "wrong_extrinsic".to_string(),
			docs: "documentation".to_string(),
			is_supported: true,
			..Default::default()
		};
		let tx = construct_extrinsic("WrongPallet", &extrinsic, vec!["0x11".to_string()]).await?;
		assert!(matches!(
			sign_and_submit_extrinsic(client, tx, ALICE_SURI).await,
			Err(Error::ExtrinsicSubmissionError(message)) if message.contains("PalletNameNotFound(\"WrongPallet\"))")
		));
		Ok(())
	}

	#[tokio::test]
	async fn construct_sudo_extrinsic_works() -> Result<()> {
		let client = set_up_client("wss://rpc1.paseo.popnetwork.xyz").await?;
		let pallets = parse_chain_metadata(&client).await?;
		let force_transfer = find_extrinsic_by_name(&pallets, "Balances", "force_transfer").await?;
		let extrinsic = construct_extrinsic(
			"Balances",
			&force_transfer,
			vec![
				"Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string(),
				"Id(5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy)".to_string(),
				"100".to_string(),
			],
		)
		.await?;
		let sudo_extrinsic = construct_sudo_extrinsic(extrinsic).await?;
		assert_eq!(sudo_extrinsic.call_name(), "sudo");
		assert_eq!(sudo_extrinsic.pallet_name(), "Sudo");
		Ok(())
	}
}
