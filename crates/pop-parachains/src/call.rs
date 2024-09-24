// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::create_signer;
use strum::{EnumMessage as _, EnumProperty as _, VariantArray as _};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};
use subxt::{
	config::DefaultExtrinsicParamsBuilder, dynamic::Value, tx::SubmittableExtrinsic, OnlineClient,
	SubstrateConfig,
};

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
pub fn supported_extrinsics(api: &OnlineClient<SubstrateConfig>) -> Vec<&Extrinsic> {
	Extrinsic::VARIANTS
		.iter()
		.filter(|t| extrinsic_is_supported(api, t.pallet(), t.extrinsic_name()))
		.collect()
}

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
	Ok(format!("{:?}", result.extrinsic_hash()))
}

fn encode_extrinsic(encoded_call_data: Vec<u8>) -> String {
	format!("0x{}", hex::encode(encoded_call_data))
}
fn decode_extrinsic(encoded_call_data: String) -> Result<Vec<u8>, Error> {
	let hex_data = encoded_call_data.trim_start_matches("0x");
	Ok(hex::decode(hex_data)?)
}

fn extrinsic_is_supported(
	api: &OnlineClient<SubstrateConfig>,
	pallet_name: &str,
	extrinsic: &str,
) -> bool {
	let metadata = api.metadata();
	// Try to get the pallet metadata by name
	let pallet_metadata = match metadata.pallet_by_name(pallet_name) {
		Some(pallet) => pallet,
		None => return false, // Return false if pallet is not found
	};
	// Try to get the extrinsic metadata by name from the pallet
	if pallet_metadata.call_variant_by_name(extrinsic).is_some() {
		return true;
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[tokio::test]
	async fn extrinsic_is_supported_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		assert!(extrinsic_is_supported(&api, "Nfts", "mint"));
		assert!(!extrinsic_is_supported(&api, "Nfts", "mint_no_exist"));
		Ok(())
	}
}
