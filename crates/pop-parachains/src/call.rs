// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::create_signer;
use scale_value::{stringify, Value};
use strum::{EnumMessage as _, EnumProperty as _, VariantArray as _};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};
use subxt::{
	config::DefaultExtrinsicParamsBuilder, tx::SubmittableExtrinsic, OnlineClient, SubstrateConfig,
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
	#[strum(serialize = "create_asset", message = "create", props(Pallet = "Assets"))]
	CreateAsset,
	#[strum(serialize = "mint_asset", message = "mint", props(Pallet = "Asset"))]
	MintAsset,
	#[strum(serialize = "create_nft", message = "create", props(Pallet = "Nfts"))]
	CreateCollection,
	#[strum(serialize = "mint", message = "mint", props(Pallet = "Nft"))]
	MintNFT,
	#[strum(serialize = "transfer", message = "transfer_allow_death", props(Pallet = "Balances"))]
	Transfer,
}
impl Extrinsic {
	/// Get the template's name.
	fn extrinsic_name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}
	/// Get the pallet of the extrinsic.
	pub fn pallet(&self) -> Result<&str, Error> {
		self.get_str("Pallet").ok_or(Error::PalletMissing)
	}
}

pub fn parse_string_into_scale_value(str: &str) -> Result<Value, Error> {
	let value = stringify::from_str(str)
		.0
		.map_err(|_| Error::ParsingValueError(str.to_string()))?;
	Ok(value)
}

pub async fn set_up_api(url: &str) -> Result<OnlineClient<SubstrateConfig>, Error> {
	let api = OnlineClient::<SubstrateConfig>::from_url(url).await?;
	Ok(api)
}

// pub async fn prepare_query(
// 	api: &OnlineClient<SubstrateConfig>,
// 	pallet_name: &str,
// 	entry_name: &str,
// 	args: Vec<Value>,
// ) -> Result<Vec<u8>, Error> {
// 	//let args = convert_vec(args_value)?;
// 	let storage = subxt::dynamic::storage(pallet_name, entry_name, args);
// 	let addr_bytes = api.storage().address_bytes(&storage)?;
//  let result = api.storage().at_latest().await?.fetch(&storage).await?;
// 	Ok(addr_bytes)
// }
fn encode_extrinsic(encoded_call_data: Vec<u8>) -> String {
	format!("0x{}", hex::encode(encoded_call_data))
}
fn decode_extrinsic(encoded_call_data: String) -> Result<Vec<u8>, Error> {
	let hex_data = encoded_call_data.trim_start_matches("0x");
	Ok(hex::decode(hex_data)?)
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
	let result = signed_extrinsic.submit_and_watch().await?.wait_for_finalized().await?;
	Ok(result.extrinsic_hash().to_string())
}

pub async fn parse_chain_metadata(metadata: Metadata) -> Result<Vec<Pallet>, Error> {
	let mut pallets: Vec<Pallet> = Vec::new();
	for pallet in metadata.pallets() {
		let extrinsics =
			pallet.call_variants().map(|variants| variants.to_vec()).unwrap_or_default(); // Return an empty Vec if Option is None
		let storage: Vec<Storage> = pallet
			.storage()
			.map(|m| {
				m.entries()
					.iter()
					.map(|entry| Storage {
						name: entry.name().to_string(),
						docs: entry.docs().concat(),
						ty: match entry.entry_type() {
							StorageEntryType::Plain(value) => (*value, None),
							StorageEntryType::Map { value_ty, key_ty, .. } => {
								(*value_ty, Some(*key_ty))
							},
						},
					})
					.collect()
			})
			.unwrap_or_default(); // Return an empty Vec if Option is None

		pallets.push(Pallet {
			label: pallet.name().to_string(),
			extrinsics,
			docs: pallet.docs().join(" "),
			storage,
		});
	}
	Ok(pallets)
}

pub fn get_type_description(
	key_ty_id: Option<u32>,
	metadata: &Metadata,
) -> Result<Vec<String>, Error> {
	if let Some(key_ty_id) = key_ty_id {
		let key_ty_description = type_description(key_ty_id, metadata.types(), false)?;
		let result = key_ty_description.trim().trim_matches(|c| c == '(' || c == ')');

		let parsed_result: Vec<String> = if result == "\"\"" {
			vec![]
		} else if !result.contains(',') {
			vec![result.to_string()]
		} else {
			result.split(',').map(|s| s.trim().to_string()).collect()
		};

		Ok(parsed_result)
	} else {
		Ok(vec![])
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use pop_common::parse_account;

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
		// let result = prepare_extrinsic(
		// 	&api,
		// 	"Nfts",
		// 	"mint",
		// 	vec![
		// 		"1".into(),
		// 		"1".into(),
		// 		"5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".into(),
		// 		"None".into(),
		// 	],
		// 	"//Alice",
		// )
		// .await?;
		let bob = parse_account("5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty")?;
		let result = prepare_extrinsic(
			&api,
			"Assets",
			"create",
			vec![
				Value::u128(3),
				Value::unnamed_variant("Id", vec![Value::from_bytes(bob)]),
				Value::u128(1000000),
			],
			"//Alice",
		)
		.await?;
		//println!("{:?}", result);
		println!("{:?}", format!("0x{}", hex::encode(result)));
		// let rs = submit_extrinsic(api, result).await?;
		// println!("{:?}", rs);
		// query("Nfts", "Collection", &metadata)?;
		// query("Nfts", "NextCollectionId", &metadata)?;

		Ok(())
	}
}
