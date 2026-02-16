// SPDX-License-Identifier: GPL-3.0

//! Metadata tests for validating type formatting functionality.

#![cfg(feature = "integration-tests")]

use anyhow::Result;
use pop_common::{format_type, test_env::SubstrateTestNode};
use subxt::{OnlineClient, SubstrateConfig};

#[tokio::test]
async fn format_type_works() -> Result<()> {
	let node = SubstrateTestNode::spawn().await?;
	let client = OnlineClient::<SubstrateConfig>::from_url(node.ws_url()).await?;
	let metadata = client.metadata();
	let registry = metadata.types();

	// Validate `Assets::create` extrinsic types cover basic cases.
	let assets_create_extrinsic = metadata
		.pallet_by_name("Assets")
		.unwrap()
		.call_variant_by_name("create")
		.unwrap();
	let assets_create_types: Vec<String> = assets_create_extrinsic
		.fields
		.iter()
		.map(|field| {
			let type_info = registry.resolve(field.ty.id).unwrap();
			format_type(type_info, registry)
		})
		.collect();
	assert_eq!(assets_create_types.len(), 3);
	assert_eq!(assets_create_types[0], "Compact<u32>"); // id
	assert_eq!(
		assets_create_types[1],
		"MultiAddress<AccountId32 ([u8;32]), ()>: Id(AccountId32 ([u8;32])), Index(Compact<()>), Raw([u8]), Address32([u8;32]), Address20([u8;20])"
	); // admin
	assert_eq!(assets_create_types[2], "u128"); // minBalance

	//  Validate `System::remark` to cover Sequences.
	let system_remark_extrinsic = metadata
		.pallet_by_name("System")
		.unwrap()
		.call_variant_by_name("remark")
		.unwrap();
	let system_remark_types: Vec<String> = system_remark_extrinsic
		.fields
		.iter()
		.map(|field| {
			let type_info = registry.resolve(field.ty.id).unwrap();
			format_type(type_info, registry)
		})
		.collect();
	assert_eq!(system_remark_types.len(), 1);
	assert_eq!(system_remark_types[0], "[u8]"); // remark

	// Extrinsic System::set_storage, cover tuples.
	let system_set_storage_extrinsic = metadata
		.pallet_by_name("System")
		.unwrap()
		.call_variant_by_name("set_storage")
		.unwrap();
	let system_set_storage_types: Vec<String> = system_set_storage_extrinsic
		.fields
		.iter()
		.map(|field| {
			let type_info = registry.resolve(field.ty.id).unwrap();
			format_type(type_info, registry)
		})
		.collect();
	assert_eq!(system_set_storage_types.len(), 1);
	assert_eq!(system_set_storage_types[0], "[([u8], [u8])]"); // 0

	Ok(())
}
