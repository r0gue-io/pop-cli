// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, Function};
use pop_common::{create_signer};
use subxt::{
	dynamic::Value,
	tx::{DynamicPayload, Payload},
	OnlineClient, SubstrateConfig,
};
use pop_common::call::TokenMetadata;

pub mod metadata;

/// Sets up an [OnlineClient] instance for connecting to a blockchain.
///
/// # Arguments
/// * `url` - Endpoint of the node.
pub async fn set_up_client(url: &str) -> Result<OnlineClient<SubstrateConfig>, Error> {
	OnlineClient::<SubstrateConfig>::from_url(url)
		.await
		.map_err(|e| Error::ConnectionFailure(e.to_string()))
}

/// Constructs a dynamic extrinsic payload for a specified dispatchable function.
///
/// # Arguments
/// * `function` - A dispatchable function.
/// * `args` - A vector of string arguments to be passed to construct the extrinsic.
pub fn construct_extrinsic(
	function: &Function,
	args: Vec<String>,
) -> Result<DynamicPayload, Error> {
	let parsed_args: Vec<Value> = metadata::parse_dispatchable_arguments(&function.params, args)?;
	Ok(subxt::dynamic::tx(function.pallet.clone(), function.name.clone(), parsed_args))
}

/// Constructs a Sudo extrinsic.
///
/// # Arguments
/// * `xt`: The extrinsic representing the dispatchable function call to be dispatched with `Root`
///   privileges.
pub fn construct_sudo_extrinsic(xt: DynamicPayload) -> Result<DynamicPayload, Error> {
	Ok(subxt::dynamic::tx("Sudo", "sudo", [xt.into_value()].to_vec()))
}

/// Signs and submits a given extrinsic.
///
/// # Arguments
/// * `client` - The client used to interact with the chain.
/// * `xt` - The extrinsic to be signed and submitted.
/// * `suri` - The secret URI (e.g., mnemonic or private key) for signing the extrinsic.
pub async fn sign_and_submit_extrinsic(
	client: &OnlineClient<SubstrateConfig>,
	url: &url::Url,
	xt: DynamicPayload,
	suri: &str,
) -> Result<String, Error> {
	let signer = create_signer(suri)?;
	let result = client
		.tx()
		.sign_and_submit_then_watch_default(&xt, &signer)
		.await
		.map_err(|e| Error::ExtrinsicSubmissionError(format!("{:?}", e)))?
		.wait_for_finalized_success()
		.await
		.map_err(|e| Error::ExtrinsicSubmissionError(format!("{:?}", e)))?;

	let metadata = client.metadata();
	let display_events =
		pop_common::call::DisplayEvents::from_events::<SubstrateConfig, pop_common::call::DefaultEnvironment>(&result, None, &metadata)?;
	let token_metadata = TokenMetadata::query::<SubstrateConfig>(url).await?;
	let events =
		display_events.display_events::<pop_common::call::DefaultEnvironment>(pop_common::call::Verbosity::Default, &token_metadata)?;

	Ok(format!(
		"Extrinsic Submitted with hash: {:?}\n\n{}",
		result.extrinsic_hash(),
		events,
	))
}

/// Encodes the call data for a given extrinsic into a hexadecimal string.
///
/// # Arguments
/// * `client` - The client used to interact with the chain.
/// * `xt` - The extrinsic whose call data will be encoded and returned.
pub fn encode_call_data(
	client: &OnlineClient<SubstrateConfig>,
	xt: &DynamicPayload,
) -> Result<String, Error> {
	let call_data = xt
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
	client: &OnlineClient<SubstrateConfig>,
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
	use crate::{find_dispatchable_by_name, parse_chain_metadata, set_up_client};
	use anyhow::Result;
	use scale_info::scale::Compact;
	use scale_info::TypeInfo;

	//----
	use subxt::events::{Events, EventDetails, Phase};
	use subxt::ext::codec::{Encode, Decode};
	use subxt::ext::{scale_decode, scale_encode};
	use subxt::Metadata;
	use subxt::utils::H256;
	use pop_common::Config;

	#[derive(
		Encode,
		Decode,
		TypeInfo,
		Clone,
		Debug,
		PartialEq,
		Eq,
		scale_encode::EncodeAsType,
		scale_decode::DecodeAsType,
	)]
	pub enum AllEvents<Ev> {
		Test(Ev),
	}
	#[derive(Encode)]
	pub struct EventRecord<E: Encode> {
		phase: Phase,
		event: AllEvents<E>,
		topics: Vec<<SubstrateConfig as Config>::Hash>,
	}

	pub fn events_raw(
		metadata: Metadata,
		event_bytes: Vec<u8>,
		num_events: u32,
	) -> Events<SubstrateConfig> {
		// Prepend compact encoded length to event bytes:
		let mut all_event_bytes = Compact(num_events).encode();
		all_event_bytes.extend(event_bytes);
		Events::decode_from(all_event_bytes, metadata)
	}

	fn events<E: Decode + Encode>(
		metadata: Metadata,
		event_records: Vec<EventRecord<E>>,
	) -> Events<SubstrateConfig> {
		let num_events = event_records.len() as u32;
		let mut event_bytes = Vec::new();
		for ev in event_records {
			ev.encode_to(&mut event_bytes);
		}
		events_raw(metadata, event_bytes, num_events)
	}

	#[test]
	fn test_filter_events() {
		let events = ExtrinsicEvents::<SubstrateConfig> {
			ext_hash: H256(),
			idx: 0,
			events: events::<TestEvent>()

		};
	}
	//----


	const ALICE_SURI: &str = "//Alice";
	pub(crate) const POP_NETWORK_TESTNET_URL: &str = "wss://rpc1.paseo.popnetwork.xyz";

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
		let pallets = parse_chain_metadata(&client)?;
		let transfer_allow_death =
			find_dispatchable_by_name(&pallets, "Balances", "transfer_allow_death")?;

		// Wrong parameters
		assert!(matches!(
			construct_extrinsic(
				&transfer_allow_death,
				vec![ALICE_SURI.to_string(), "100".to_string()],
			),
			Err(Error::ParamProcessingError)
		));
		// Valid parameters
		let xt = construct_extrinsic(
			&transfer_allow_death,
			vec![
				"Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string(),
				"100".to_string(),
			],
		)?;
		assert_eq!(xt.call_name(), "transfer_allow_death");
		assert_eq!(xt.pallet_name(), "Balances");
		Ok(())
	}

	#[tokio::test]
	async fn encode_call_data_works() -> Result<()> {
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let pallets = parse_chain_metadata(&client)?;
		let remark = find_dispatchable_by_name(&pallets, "System", "remark")?;
		let xt = construct_extrinsic(&remark, vec!["0x11".to_string()])?;
		assert_eq!(encode_call_data(&client, &xt)?, "0x00000411");
		let xt = construct_extrinsic(&remark, vec!["123".to_string()])?;
		assert_eq!(encode_call_data(&client, &xt)?, "0x00000c313233");
		let xt = construct_extrinsic(&remark, vec!["test".to_string()])?;
		assert_eq!(encode_call_data(&client, &xt)?, "0x00001074657374");
		Ok(())
	}

	#[tokio::test]
	async fn decode_call_data_works() -> Result<()> {
		assert!(matches!(decode_call_data("wrongcalldata"), Err(Error::CallDataDecodingError(..))));
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let pallets = parse_chain_metadata(&client)?;
		let remark = find_dispatchable_by_name(&pallets, "System", "remark")?;
		let xt = construct_extrinsic(&remark, vec!["0x11".to_string()])?;
		let expected_call_data = xt.encode_call_data(&client.metadata())?;
		assert_eq!(decode_call_data("0x00000411")?, expected_call_data);
		Ok(())
	}

	#[tokio::test]
	async fn sign_and_submit_wrong_extrinsic_fails() -> Result<()> {
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let function = Function {
			pallet: "WrongPallet".to_string(),
			name: "wrong_extrinsic".to_string(),
			index: 0,
			docs: "documentation".to_string(),
			is_supported: true,
			..Default::default()
		};
		let xt = construct_extrinsic(&function, vec!["0x11".to_string()])?;
		assert!(matches!(
			sign_and_submit_extrinsic(&client, xt, ALICE_SURI).await,
			Err(Error::ExtrinsicSubmissionError(message)) if message.contains("PalletNameNotFound(\"WrongPallet\"))")
		));
		Ok(())
	}

	#[tokio::test]
	async fn construct_sudo_extrinsic_works() -> Result<()> {
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let pallets = parse_chain_metadata(&client)?;
		let force_transfer = find_dispatchable_by_name(&pallets, "Balances", "force_transfer")?;
		let xt = construct_extrinsic(
			&force_transfer,
			vec![
				"Id(5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty)".to_string(),
				"Id(5DAAnrj7VHTznn2AWBemMuyBwZWs6FNFjdyVXUeYum3PTXFy)".to_string(),
				"100".to_string(),
			],
		)?;
		let xt = construct_sudo_extrinsic(xt)?;
		assert_eq!(xt.call_name(), "sudo");
		assert_eq!(xt.pallet_name(), "Sudo");
		Ok(())
	}
}
