// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, Function, Param};
use metadata::params::field_to_param;
use pop_common::{
	call::{DefaultEnvironment, DisplayEvents, TokenMetadata, Verbosity},
	create_signer,
};
use sp_core::bytes::{from_hex, to_hex};
use subxt::{
	blocks::ExtrinsicEvents,
	dynamic::Value,
	tx::{DynamicPayload, Payload, SubmittableExtrinsic},
	OnlineClient, SubstrateConfig,
};
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
pub fn construct_sudo_extrinsic(xt: DynamicPayload) -> DynamicPayload {
	subxt::dynamic::tx("Sudo", "sudo", [xt.into_value()].to_vec())
}

/// Constructs a Proxy call extrinsic.
///
/// # Arguments
/// * `client` - The client used to interact with the chain.
/// * `proxy_account` - The proxied account that will execute the extrinsic.
/// * `xt`: The extrinsic representing the dispatchable function call to be dispatched using the
///   proxy.
pub fn construct_proxy_extrinsic(
	client: &OnlineClient<SubstrateConfig>,
	proxy_account: String,
	xt: DynamicPayload,
) -> Result<DynamicPayload, Error> {
	let metadata = client.metadata();
	let proxy_call_fields: Vec<Param> = metadata
		.pallet_by_name("Proxy")
		.and_then(|p| p.call_variant_by_name("proxy"))
		.map(|c| {
			c.fields
				.iter()
				.filter(|f| {
					// Skip fields where type_name contains "RuntimeCall"
					f.type_name.as_deref().map_or(true, |name| !name.contains("RuntimeCall"))
				})
				.map(|f| field_to_param(&metadata, f))
				.collect::<Result<Vec<Param>, Error>>()
		})
		.ok_or_else(|| Error::MetadataParsingError("Proxy call".to_string()))??;
	let parsed_args: Vec<Value> = metadata::parse_dispatchable_arguments(
		&proxy_call_fields,
		vec![proxy_account, "None()".to_string()],
	)?;

	Ok(subxt::dynamic::tx(
		"Proxy",
		"proxy",
		[parsed_args[0].clone(), parsed_args[1].clone(), xt.into_value()].to_vec(),
	))
}

/// Signs and submits a given extrinsic.
///
/// # Arguments
/// * `client` - The client used to interact with the chain.
/// * `url` - Endpoint of the node.
/// * `xt` - The (encoded) extrinsic to be signed and submitted.
/// * `suri` - The secret URI (e.g., mnemonic or private key) for signing the extrinsic.
pub async fn sign_and_submit_extrinsic<Xt: Payload>(
	client: &OnlineClient<SubstrateConfig>,
	url: &url::Url,
	xt: Xt,
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

	// Obtain required metadata and parse events. The following is using existing logic from
	// `cargo-contract`, also used in calling contracts, due to simplicity and can be refactored in
	// the future.
	let metadata = client.metadata();
	let token_metadata = TokenMetadata::query::<SubstrateConfig>(url).await?;
	let events = DisplayEvents::from_events::<SubstrateConfig, DefaultEnvironment>(
		&result, None, &metadata,
	)?;
	let events =
		events.display_events::<DefaultEnvironment>(Verbosity::Default, &token_metadata)?;

	Ok(format!("Extrinsic Submitted with hash: {:?}\n\n{}", result.extrinsic_hash(), events))
}

/// Submits a signed extrinsic.
///
/// # Arguments
/// * `client` - The client used to interact with the chain.
/// * `payload` - The signed payload string to be submitted.
pub async fn submit_signed_extrinsic(
	client: OnlineClient<SubstrateConfig>,
	payload: String,
) -> Result<ExtrinsicEvents<SubstrateConfig>, Error> {
	let hex_encoded =
		from_hex(&payload).map_err(|e| Error::CallDataDecodingError(e.to_string()))?;
	let extrinsic = SubmittableExtrinsic::from_bytes(client, hex_encoded);
	extrinsic
		.submit_and_watch()
		.await
		.map_err(|e| Error::ExtrinsicSubmissionError(format!("{:?}", e)))?
		.wait_for_finalized_success()
		.await
		.map_err(|e| Error::ExtrinsicSubmissionError(format!("{:?}", e)))
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
	Ok(to_hex(&call_data, false))
}

/// Decodes a hex-encoded string into a vector of bytes representing the call data.
///
/// # Arguments
/// * `call_data` - The hex-encoded string representing call data.
pub fn decode_call_data(call_data: &str) -> Result<Vec<u8>, Error> {
	from_hex(call_data).map_err(|e| Error::CallDataDecodingError(e.to_string()))
}

/// This struct implements the [`Payload`] trait and is used to submit
/// pre-encoded SCALE call data directly, without the dynamic construction of transactions.
pub struct CallData(Vec<u8>);

impl CallData {
	/// Create a new instance of `CallData`.
	pub fn new(data: Vec<u8>) -> CallData {
		CallData(data)
	}
}

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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{find_dispatchable_by_name, parse_chain_metadata, set_up_client};
	use anyhow::Result;
	use url::Url;

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
	async fn construct_proxy_extrinsic_work() -> Result<()> {
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let pallets = parse_chain_metadata(&client)?;
		let remark_dispatchable = find_dispatchable_by_name(&pallets, "System", "remark")?;
		let remark = construct_extrinsic(remark_dispatchable, ["0x11".to_string()].to_vec())?;
		let xt = construct_proxy_extrinsic(
			&client,
			"Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string(),
			remark,
		)?;
		// Encoded call data for a proxy extrinsic with remark as the call.
		// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frpc1.paseo.popnetwork.xyz#/extrinsics/decode/0x29000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de0000000411
		assert_eq!(
			encode_call_data(&client, &xt)?,
			"0x29000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de0000000411"
		);
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
			sign_and_submit_extrinsic(&client, &Url::parse(POP_NETWORK_TESTNET_URL)?, xt, ALICE_SURI).await,
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
		let xt = construct_sudo_extrinsic(xt);
		assert_eq!(xt.call_name(), "sudo");
		assert_eq!(xt.pallet_name(), "Sudo");
		Ok(())
	}
}
