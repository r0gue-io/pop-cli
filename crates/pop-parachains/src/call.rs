// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use clap::builder::Str;
use pop_common::create_signer;
use scale_info::form::PortableForm;
use strum::{EnumMessage as _, EnumProperty as _, VariantArray as _};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};
use subxt::{
	config::DefaultExtrinsicParamsBuilder,
	dynamic::Value,
	tx::{Payload, SubmittableExtrinsic},
	OnlineClient, SubstrateConfig,
};
/// A supported pallet.
#[derive(AsRefStr, Clone, Debug, Display, EnumMessage, EnumString, Eq, PartialEq, VariantArray)]
pub enum Pallet {
	//Assets.
	#[strum(serialize = "Assets")]
	Assets,
	//Balances.
	#[strum(serialize = "Balances")]
	Balances,
	/// NFT.
	#[strum(serialize = "Nfts")]
	Nfts,
}
impl Pallet {
	/// Get the list of extrinsics available.
	pub fn extrinsics(&self) -> Vec<&Extrinsic> {
		Extrinsic::VARIANTS
			.iter()
			.filter(|t| t.get_str("Pallet") == Some(self.as_ref()))
			.collect()
	}
}

#[derive(
	AsRefStr,
	Clone,
	Debug,
	Display,
	EnumMessage,
	EnumProperty,
	EnumString,
	Eq,
	PartialEq,
	VariantArray,
)]
pub enum Extrinsic {
	#[strum(
		serialize = "create_asset",
		message = "create",
		detailed_message = "Create an Asset",
		props(Pallet = "Assets")
	)]
	CreateAsset,
	#[strum(
		serialize = "mint_asset",
		message = "mint",
		detailed_message = "Mint an Asset",
		props(Pallet = "Assets")
	)]
	MintAsset,
	#[strum(
		serialize = "create_nft",
		message = "create",
		detailed_message = "Create a NFT Collection",
		props(Pallet = "Nfts")
	)]
	CreateCollection,
	#[strum(
		serialize = "mint",
		message = "mint",
		detailed_message = "Mint a NFT",
		props(Pallet = "Nfts")
	)]
	MintNFT,
	#[strum(
		serialize = "transfer",
		message = "transfer_allow_death",
		detailed_message = "Transfer",
		props(Pallet = "Balances")
	)]
	Transfer,
}
impl Extrinsic {
	/// Get the template's name.
	pub fn extrinsic_name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}
	/// Get the description of the extrinsic.
	pub fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the pallet of the extrinsic.
	pub fn pallet(&self) -> &str {
		self.get_str("Pallet").unwrap_or_default()
	}
}

// pub fn parse_string_into_scale_value(str: &str) -> Result<Value, Error> {
// 	let value = stringify::from_str(str)
// 		.0
// 		.map_err(|_| Error::ParsingValueError(str.to_string()))?;
// 	Ok(value)
// }

pub async fn set_up_api(url: &str) -> Result<OnlineClient<SubstrateConfig>, Error> {
	let api = OnlineClient::<SubstrateConfig>::from_url(url).await?;
	Ok(api)
}

pub async fn prepare_extrinsic(
	api: &OnlineClient<SubstrateConfig>,
	pallet_name: &str,
	entry_name: &str,
	args_value: Vec<Value>,
	suri: &str,
) -> Result<String, Error> {
	let signer = create_signer(suri)?;
	let tx = subxt::dynamic::tx(pallet_name, entry_name, args_value);
	let signed_extrinsic: SubmittableExtrinsic<SubstrateConfig, OnlineClient<SubstrateConfig>> =
		api.tx()
			.create_signed(&tx, &signer, DefaultExtrinsicParamsBuilder::new().build())
			.await?;
	Ok(encode_extrinsic(signed_extrinsic.encoded().to_vec()))
}

pub async fn submit_extrinsic(
	api: OnlineClient<SubstrateConfig>,
	encoded_extrinsic: String,
) -> Result<String, Error> {
	let extrinsic = decode_extrinsic(encoded_extrinsic)?;
	let signed_extrinsic: SubmittableExtrinsic<SubstrateConfig, OnlineClient<SubstrateConfig>> =
		SubmittableExtrinsic::from_bytes(api, extrinsic);
	let result = signed_extrinsic.submit_and_watch().await?;
	Ok(result.extrinsic_hash().to_string())
}

fn encode_extrinsic(encoded_call_data: Vec<u8>) -> String {
	format!("0x{}", hex::encode(encoded_call_data))
}
fn decode_extrinsic(encoded_call_data: String) -> Result<Vec<u8>, Error> {
	let hex_data = encoded_call_data.trim_start_matches("0x");
	Ok(hex::decode(hex_data)?)
}

pub fn fetch_types(
	api: &OnlineClient<SubstrateConfig>,
	pallet_name: &str,
	extrinsic: &str,
) -> Result<String, Error> {
	let metadata = api.metadata();
	let pallet_metadata = metadata
		.pallet_by_name(pallet_name)
		.ok_or(Error::PalletNotFound(pallet_name.to_string()))?;
	let extrinsic_metadata = pallet_metadata
		.call_variant_by_name(extrinsic)
		.ok_or(Error::PalletNotFound(pallet_name.to_string()))?;
	//println!("{:?}", extrinsic_metadata.fields);
	Ok("".to_string())
}

#[cfg(test)]
mod tests {
	use std::vec;

	use super::*;
	use anyhow::Result;
	use pop_common::parse_account;
	use subxt::ext::{
		scale_encode::EncodeAsType,
		scale_value::{self, value, Composite, Variant},
	};

	#[tokio::test]
	async fn fetch_works() -> Result<()> {
		let api = set_up_api("ws://127.0.0.1:53677").await?;
		let a = fetch_types(&api, "Nfts", "mint")?;
		let me = api.metadata();
		let ty = me.types().resolve(279);
		println!("TYPE {:?}", ty);
		Ok(())
	}

	// #[tokio::test]
	// async fn query_works() -> Result<()> {
	// 	let api = set_up_api("wss://rpc2.paseo.popnetwork.xyz").await?;
	// 	let result = prepare_query(&api, "Assets", "Asset", vec!["50".into()]).await?;
	// 	println!("{:?}", result);
	// 	// query("Nfts", "Collection", &metadata)?;
	// 	// query("Nfts", "NextCollectionId", &metadata)?;

	// 	Ok(())
	// }
	#[tokio::test]
	async fn extrinsic_works() -> Result<()> {
		let api = set_up_api("ws://127.0.0.1:53677").await?;
		let bob = parse_account("5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty")?;
		let alice = parse_account("5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY")?;
		let owned_item = Value::unnamed_variant("Some".to_string(), vec![Value::u128(1)]);
		let mint_price = Value::unnamed_variant("Some".to_string(), vec![Value::u128(1)]);
		let mint_witness = Value::unnamed_composite(vec![owned_item, mint_price]);

		let some = Value::unnamed_variant("Some".to_string(), vec![mint_witness]);

		let ni = Value::unnamed_variant(
			"None",
			vec![], // No fields for `None`
		);
		// let result = prepare_extrinsic(
		// 	&api,
		// 	"Nfts",
		// 	"mint",
		// 	vec![
		// 		Value::u128(1),
		// 		Value::u128(1),
		// 		Value::unnamed_variant("Id", vec![Value::from_bytes(bob)]),
		// 		ni,
		// 	],
		// 	"//Alice",
		// )
		// .await?;

		let max_supply = Value::unnamed_variant("Some".to_string(), vec![Value::u128(1)]);
		let mint_type = Value::unnamed_variant("Issuer".to_string(), vec![]);
		let price = Value::unnamed_variant("Some".to_string(), vec![Value::u128(1)]);
		let start_block = Value::unnamed_variant("Some".to_string(), vec![Value::u128(1)]);
		let end_block = Value::unnamed_variant("Some".to_string(), vec![Value::u128(1)]);
		let mint_settings = Value::unnamed_composite(vec![
			mint_type,
			price,
			start_block,
			end_block,
			Value::u128(1),
		]);
		let config_collection =
			Value::unnamed_composite(vec![Value::u128(1), max_supply, mint_settings]);
		let result2 = prepare_extrinsic(
			&api,
			"Nfts",
			"create",
			vec![Value::unnamed_variant("Id", vec![Value::from_bytes(alice)]), config_collection],
			"//Alice",
		)
		.await?;
		//println!("{:?}", format!("0x{}", hex::encode(result)));
		println!("{:?}", format!("0x{}", hex::encode(result2)));
		//let rs = submit_extrinsic(api, result).await?;
		//println!("{:?}", rs);
		// query("Nfts", "Collection", &metadata)?;
		// query("Nfts", "NextCollectionId", &metadata)?;

		Ok(())
	}
}
