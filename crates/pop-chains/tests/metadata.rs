// SPDX-License-Identifier: GPL-3.0

use anyhow::Result;
use pop_chains::{
	field_to_param, find_dispatchable_by_name, find_pallet_by_name, parse_chain_metadata,
	set_up_client, Error,
};
use pop_common::test_env::TestNode;

#[tokio::test]
async fn parse_chain_metadata_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	// Test the first pallet is parsed correctly
	let first_pallet = pallets.first().unwrap();
	assert_eq!(first_pallet.name, "System");
	assert_eq!(first_pallet.index, 0);
	assert_eq!(first_pallet.docs, "");
	assert_eq!(first_pallet.functions.len(), 11);
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
	let node = TestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	assert!(matches!(
        find_pallet_by_name(&pallets, "WrongName"),
        Err(Error::PalletNotFound(pallet)) if pallet == "WrongName".to_string()));
	let pallet = find_pallet_by_name(&pallets, "Balances")?;
	assert_eq!(pallet.name, "Balances");
	assert_eq!(pallet.functions.len(), 9);
	Ok(())
}

#[tokio::test]
async fn find_dispatchable_by_name_works() -> Result<()> {
	let node = TestNode::spawn().await?;
	let client = set_up_client(node.ws_url()).await?;
	let pallets = parse_chain_metadata(&client)?;
	assert!(matches!(
        find_dispatchable_by_name(&pallets, "WrongName", "wrong_name"),
        Err(Error::PalletNotFound(pallet)) if pallet == "WrongName".to_string()));
	assert!(matches!(
		find_dispatchable_by_name(&pallets, "Balances", "wrong_name"),
		Err(Error::FunctionNotSupported)
	));
	let function = find_dispatchable_by_name(&pallets, "Balances", "force_transfer")?;
	assert_eq!(function.name, "force_transfer");
	assert_eq!(function.docs, "Exactly as `transfer_allow_death`, except the origin must be root and the source account may be specified.");
	assert_eq!(function.is_supported, true);
	assert_eq!(function.params.len(), 3);
	Ok(())
}

#[tokio::test]
async fn field_to_param_works() -> Result<()> {
	let node = TestNode::spawn().await?;
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
	assert_eq!(params.first().unwrap().type_name, "MultiAddress<AccountId32 ([u8;32]),()>: Id(AccountId32 ([u8;32])), Index(Compact<()>), Raw([u8]), Address32([u8;32]), Address20([u8;20])");
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
		field_to_param(&metadata, &function.fields.first().unwrap()),
		Err(Error::FunctionNotSupported)
	));
	let function = metadata
		.pallet_by_name("Utility")
		.unwrap()
		.call_variant_by_name("batch")
		.unwrap();
	assert!(matches!(
		field_to_param(&metadata, &function.fields.first().unwrap()),
		Err(Error::FunctionNotSupported)
	));

	Ok(())
}
