// SPDX-License-Identifier: GPL-3.0

//! Integration tests for the pop-chains crate functionality.

#![cfg(feature = "integration-tests")]

use anyhow::Result;
use pop_chains::{
	Error, Function, Payload, construct_extrinsic, construct_proxy_extrinsic, decode_call_data,
	encode_call_data, field_to_param, find_callable_by_name, find_pallet_by_name,
	parse_chain_metadata, set_up_client, sign_and_submit_extrinsic,
};
use pop_common::test_env::SubstrateTestNode;
use std::time::Duration;
use subxt::{OnlineClient, SubstrateConfig};
use url::Url;

const ALICE_SURI: &str = "//Alice";
const POLKADOT_NETWORK_URLS: [&str; 3] = [
	"wss://rpc.polkadot.io",
	"wss://polkadot-rpc.dwellir.com",
	"wss://polkadot.api.onfinality.io/public-ws",
];

async fn set_up_client_any(urls: &[&str]) -> Result<OnlineClient<SubstrateConfig>> {
	let mut last_error: Option<anyhow::Error> = None;
	for url in urls {
		let attempt = tokio::time::timeout(Duration::from_secs(30), set_up_client(url)).await;
		match attempt {
			Ok(Ok(client)) => return Ok(client),
			Ok(Err(err)) => {
				last_error = Some(anyhow::anyhow!("Failed to connect to {url}: {err}"));
			},
			Err(_) => {
				last_error = Some(anyhow::anyhow!("Timed out connecting to {url}"));
			},
		}
	}
	Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No RPC endpoints provided")))
}

#[tokio::test]
async fn construct_proxy_extrinsic_work() -> Result<()> {
	let client = set_up_client_any(&POLKADOT_NETWORK_URLS).await?;
	let pallets = parse_chain_metadata(&client)?;
	let call_item = find_callable_by_name(&pallets, "System", "remark")?;
	let remark_dispatchable = call_item.as_function().unwrap();
	let remark = construct_extrinsic(remark_dispatchable, ["0x11".to_string()].to_vec())?;
	let xt = construct_proxy_extrinsic(
		&pallets,
		"Id(13czcAAt6xgLwZ8k6ZpkrRL5V2pjKEui3v9gHAN9PoxYZDbf)".to_string(),
		remark,
	)?;
	// Encoded call data for a proxy extrinsic with remark as the call.
	// Reference: https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot-rpc.publicnode.com#/extrinsics/decode/0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de0000000411
	assert_eq!(
		encode_call_data(&client, &xt)?,
		"0x1d000073ebf9c947490b9170ea4fd3031ae039452e428531317f76bf0a02124f8166de0000000411"
	);
	Ok(())
}

#[tokio::test]
async fn encode_and_decode_call_data_works() -> Result<()> {
	let node = SubstrateTestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	let call_item = find_callable_by_name(&pallets, "System", "remark")?;
	let remark = call_item.as_function().unwrap();
	let xt = construct_extrinsic(remark, vec!["0x11".to_string()])?;
	let encoded = encode_call_data(&client, &xt)?;
	assert_eq!(decode_call_data(&encoded)?, xt.encode_call_data(&client.metadata())?);
	let xt = construct_extrinsic(remark, vec!["123".to_string()])?;
	let encoded = encode_call_data(&client, &xt)?;
	assert_eq!(decode_call_data(&encoded)?, xt.encode_call_data(&client.metadata())?);
	let xt = construct_extrinsic(remark, vec!["test".to_string()])?;
	let encoded = encode_call_data(&client, &xt)?;
	assert_eq!(decode_call_data(&encoded)?, xt.encode_call_data(&client.metadata())?);
	Ok(())
}

#[tokio::test]
async fn sign_and_submit_wrong_extrinsic_fails() -> Result<()> {
	let node = SubstrateTestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
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
		sign_and_submit_extrinsic(&client, &Url::parse(node.ws_url())?, xt, ALICE_SURI).await,
		Err(Error::ExtrinsicSubmissionError(message)) if message.contains("PalletNameNotFound(\"WrongPallet\"))")
	));
	Ok(())
}

#[tokio::test]
async fn parse_chain_metadata_works() -> Result<()> {
	let node = SubstrateTestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	// Test the first pallet is parsed correctly
	let first_pallet = pallets.first().unwrap();
	assert_eq!(first_pallet.name, "System");
	assert_eq!(first_pallet.index, 0);
	assert_eq!(first_pallet.docs, "");
	assert!(first_pallet.functions.len() > 0);
	let first_function = first_pallet.functions.first().unwrap();
	assert_eq!(first_function.name, "remark");
	assert_eq!(first_function.index, 0);
	assert_eq!(
		first_function.docs,
		"Make some on-chain remark. Can be executed by every `origin`."
	);
	assert!(first_function.is_supported);
	assert_eq!(first_function.params.first().unwrap().name, "remark");
	assert_eq!(first_function.params.first().unwrap().type_name, "[u8]");
	assert_eq!(first_function.params.first().unwrap().sub_params.len(), 0);
	assert!(!first_function.params.first().unwrap().is_optional);
	assert!(!first_function.params.first().unwrap().is_tuple);
	assert!(!first_function.params.first().unwrap().is_variant);
	assert!(first_function.params.first().unwrap().is_sequence);
	Ok(())
}

#[tokio::test]
async fn find_pallet_by_name_works() -> Result<()> {
	let node = SubstrateTestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	assert!(matches!(
        find_pallet_by_name(&pallets, "WrongName"),
        Err(Error::PalletNotFound(pallet)) if pallet == *"WrongName"));
	let pallet = find_pallet_by_name(&pallets, "Balances")?;
	assert_eq!(pallet.name, "Balances");
	assert!(pallet.functions.len() > 0);
	Ok(())
}

#[tokio::test]
async fn find_dispatchable_by_name_works() -> Result<()> {
	let node = SubstrateTestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	assert!(matches!(
        find_callable_by_name(&pallets, "WrongName", "wrong_name"),
        Err(Error::PalletNotFound(pallet)) if pallet == *"WrongName"));
	assert!(matches!(
		find_callable_by_name(&pallets, "Balances", "wrong_name"),
		Err(Error::FunctionNotFound(_))
	));
	let call_item = find_callable_by_name(&pallets, "Balances", "force_transfer")?;
	let function = call_item.as_function().unwrap();
	assert_eq!(function.name, "force_transfer");
	assert_eq!(
		function.docs,
		"Exactly as `transfer_allow_death`, except the origin must be root and the source account may be specified."
	);
	assert!(function.is_supported);
	assert_eq!(function.params.len(), 3);
	Ok(())
}

#[tokio::test]
async fn field_to_param_works() -> Result<()> {
	let node = SubstrateTestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let metadata = client.metadata();
	// Test a supported dispatchable function.
	let function = metadata
		.pallet_by_name("Balances")
		.unwrap()
		.call_variant_by_name("force_transfer")
		.unwrap();
	let mut params = Vec::new();
	for field in &function.fields {
		params.push(field_to_param(&metadata, field)?)
	}
	assert_eq!(params.len(), 3);
	assert_eq!(params.first().unwrap().name, "source");
	assert_eq!(
		params.first().unwrap().type_name,
		"MultiAddress<AccountId32 ([u8;32]), ()>: Id(AccountId32 ([u8;32])), Index(Compact<()>), Raw([u8]), Address32([u8;32]), Address20([u8;20])"
	);
	assert_eq!(params.first().unwrap().sub_params.len(), 5);
	assert_eq!(params.first().unwrap().sub_params.first().unwrap().name, "Id");
	assert_eq!(params.first().unwrap().sub_params.first().unwrap().type_name, "");
	assert_eq!(
		params
			.first()
			.unwrap()
			.sub_params
			.first()
			.unwrap()
			.sub_params
			.first()
			.unwrap()
			.name,
		"Id"
	);
	assert_eq!(
		params
			.first()
			.unwrap()
			.sub_params
			.first()
			.unwrap()
			.sub_params
			.first()
			.unwrap()
			.type_name,
		"AccountId32 ([u8;32])"
	);
	// Test some dispatchable functions that are not supported.
	let function = metadata.pallet_by_name("Sudo").unwrap().call_variant_by_name("sudo").unwrap();
	assert!(matches!(
		field_to_param(&metadata, function.fields.first().unwrap()),
		Err(Error::CallableNotSupported)
	));
	let function = metadata
		.pallet_by_name("Utility")
		.unwrap()
		.call_variant_by_name("batch")
		.unwrap();
	assert!(matches!(
		field_to_param(&metadata, function.fields.first().unwrap()),
		Err(Error::CallableNotSupported)
	));

	Ok(())
}
