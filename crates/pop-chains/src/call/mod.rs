// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, find_dispatchable_by_name, Function, Pallet, Param};
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
/// * `pallets`: List of pallets available within the chain's runtime.
/// * `proxied_account` - The account on whose behalf the proxy will act.
/// * `xt`: The extrinsic representing the dispatchable function call to be dispatched using the
///   proxy.
pub fn construct_proxy_extrinsic(
	pallets: &[Pallet],
	proxied_account: String,
	xt: DynamicPayload,
) -> Result<DynamicPayload, Error> {
	let proxy_function = find_dispatchable_by_name(pallets, "Proxy", "proxy")?;
	// `find_dispatchable_by_name` doesn't support parsing parameters that are calls.
	// Therefore, we only parse the first two parameters for the proxy call
	// using `parse_dispatchable_arguments`, while the last parameter (which is the call)
	// must be manually added.
	let required_params: Vec<Param> = proxy_function.params.iter().take(2).cloned().collect();
	let mut parsed_args: Vec<Value> = metadata::parse_dispatchable_arguments(
		&required_params,
		vec![proxied_account, "None()".to_string()],
	)?;
	let real = parsed_args.remove(0);
	let proxy_type = parsed_args.remove(0);
	Ok(subxt::dynamic::tx("Proxy", "proxy", [real, proxy_type, xt.into_value()].to_vec()))
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

	let events = parse_and_format_events(client, url, &result).await?;

	Ok(format!("Extrinsic Submitted with hash: {:?}\n\n{}", result.extrinsic_hash(), events))
}

/// Parses and formats the events from the extrinsic result.
///
/// # Arguments
/// * `client` - The client used to interact with the chain.
/// * `url` - Endpoint of the node.
/// * `result` - The extrinsic result from which to extract events.
pub async fn parse_and_format_events(
	client: &OnlineClient<SubstrateConfig>,
	url: &url::Url,
	result: &ExtrinsicEvents<SubstrateConfig>,
) -> Result<String, Error> {
	// Obtain required metadata and parse events. The following is using existing logic from
	// `cargo-contract`, also used in calling contracts, due to simplicity and can be refactored in
	// the future.
	let metadata = client.metadata();
	let token_metadata = TokenMetadata::query::<SubstrateConfig>(url).await?;
	let events =
		DisplayEvents::from_events::<SubstrateConfig, DefaultEnvironment>(result, None, &metadata)?;
	let events =
		events.display_events::<DefaultEnvironment>(Verbosity::Default, &token_metadata)?;

	Ok(events)
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
	use crate::set_up_client;
	use anyhow::Result;

	const ALICE_SURI: &str = "//Alice";

	#[tokio::test]
	async fn set_up_client_fails_wrong_url() -> Result<()> {
		assert!(matches!(
			set_up_client("wss://wronguri.xyz").await,
			Err(Error::ConnectionFailure(_))
		));
		Ok(())
	}

	#[tokio::test]
	async fn construct_extrinsic_works() -> Result<()> {
		let transfer_allow_death= Function { pallet: "Balances".into(), name: "transfer_allow_death".into(), index: 0, docs: ".".into(), params: [Param { name: "dest".into(), type_name: "MultiAddress<AccountId32 ([u8;32]),()>: Id(AccountId32 ([u8;32])), Index(Compact<()>), Raw([u8]), Address32([u8;32]), Address20([u8;20])".into(), sub_params: [Param { name: "Id".into(), type_name: "".into(), sub_params: [Param { name: "Id".into(), type_name: "AccountId32 ([u8;32])".into(), sub_params: [Param { name: "Id".into(), type_name: "[u8;32]".into(), sub_params: [].to_vec(), ..Default::default() }].to_vec(), ..Default::default() }].to_vec(), ..Default::default() }].to_vec(), ..Default::default() },Param { name: "value".into(), type_name: "Compact<u128>".into(), sub_params: [].to_vec(), ..Default::default() }].to_vec(), is_supported: true };
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
	async fn construct_sudo_extrinsic_works() -> Result<()> {
		let xt = construct_extrinsic(
			&Function::default(),
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
