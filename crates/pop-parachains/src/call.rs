// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::{create_signer, parse_account};
use strum::{EnumMessage as _, EnumProperty as _, VariantArray as _};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};
use subxt::{
	dynamic::Value,
	tx::{DynamicPayload, Payload},
	OnlineClient, SubstrateConfig,
};

#[derive(AsRefStr, Clone, Debug, Display, EnumMessage, EnumString, Eq, PartialEq, VariantArray)]
pub enum Pallet {
	#[strum(serialize = "Assets")]
	Assets,
	#[strum(serialize = "Balances")]
	Balances,
	#[strum(serialize = "Nfts")]
	Nfts,
}

#[derive(
	AsRefStr,
	Clone,
	Debug,
	Display,
	EnumMessage,
	EnumString,
	EnumProperty,
	Eq,
	PartialEq,
	VariantArray,
)]
pub enum Extrinsic {
	#[strum(
		serialize = "create",
		message = "create",
		detailed_message = "Create an Asset",
		props(Pallet = "Assets")
	)]
	CreateAsset,
	#[strum(
		serialize = "mint",
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
		serialize = "mint_nft",
		message = "mint",
		detailed_message = "Mint a NFT",
		props(Pallet = "Nfts")
	)]
	MintNFT,
	#[strum(
		serialize = "transfer",
		message = "transfer_allow_death",
		detailed_message = "Transfer Balance",
		props(Pallet = "Balances")
	)]
	Transfer,
}
impl Extrinsic {
	/// Get the extrinsic's name.
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

pub async fn set_up_api(url: &str) -> Result<OnlineClient<SubstrateConfig>, Error> {
	let api = OnlineClient::<SubstrateConfig>::from_url(url).await?;
	Ok(api)
}

pub fn supported_extrinsics(api: &OnlineClient<SubstrateConfig>) -> Vec<&Extrinsic> {
	Extrinsic::VARIANTS
		.iter()
		.filter(|t| extrinsic_is_supported(api, t.pallet(), t.extrinsic_name()))
		.collect()
}

pub fn construct_extrinsic(
	pallet_name: &str,
	extrinsic_name: &str,
	args: Vec<String>,
) -> Result<DynamicPayload, Error> {
	let parsed_args: Vec<Value> = parse_args(pallet_name, extrinsic_name, args)?;
	Ok(subxt::dynamic::tx(pallet_name, extrinsic_name, parsed_args))
}

pub async fn sign_and_submit_extrinsic(
	api: OnlineClient<SubstrateConfig>,
	tx: DynamicPayload,
	suri: &str,
) -> Result<String, Error> {
	let signer = create_signer(suri)?;
	let result = api
		.tx()
		.sign_and_submit_then_watch_default(&tx, &signer)
		.await?
		.wait_for_finalized()
		.await?
		.wait_for_success()
		.await?;
	Ok(format!("{:?}", result.extrinsic_hash()))
}

pub fn encode_call_data(
	api: &OnlineClient<SubstrateConfig>,
	tx: &DynamicPayload,
) -> Result<String, Error> {
	let call_data = tx.encode_call_data(&api.metadata())?;
	Ok(format!("0x{}", hex::encode(call_data)))
}

fn parse_args(
	pallet_name: &str,
	extrinsic_name: &str,
	raw_args: Vec<String>,
) -> Result<Vec<Value>, Error> {
	let mut args: Vec<Value> = Vec::new();
	let extrinsic = Extrinsic::VARIANTS
		.iter()
		.find(|t| t.pallet() == pallet_name && t.extrinsic_name() == extrinsic_name)
		.ok_or(Error::ExtrinsicNotSupported(extrinsic_name.to_string()))?;
	match extrinsic {
		Extrinsic::CreateAsset => {
			if raw_args.len() < 3 {
				return Err(Error::ParsingArgsError);
			}
			args.push(parse_u128(&raw_args[0])?);
			args.push(parse_account_id(&raw_args[1])?);
			args.push(parse_u128(&raw_args[2])?);
		},
		Extrinsic::MintAsset => {
			if raw_args.len() < 3 {
				return Err(Error::ParsingArgsError);
			}
			args.push(parse_u128(&raw_args[0])?);
			args.push(parse_account_id(&raw_args[1])?);
			args.push(parse_u128(&raw_args[2])?);
		},
		Extrinsic::CreateCollection => {
			if raw_args.len() < 7 {
				return Err(Error::ParsingArgsError);
			}
			args.push(parse_account_id(&raw_args[0])?);
			let mint_settings = Value::unnamed_composite(vec![
				Value::unnamed_variant(&raw_args[3], vec![]),
				parse_optional_u128(&raw_args[4])?,
				parse_optional_u128(&raw_args[5])?,
				parse_optional_u128(&raw_args[6])?,
				parse_u128(&raw_args[7])?,
			]);
			let max_supply = parse_optional_u128(&raw_args[2])?;
			args.push(Value::unnamed_composite(vec![
				parse_u128(&raw_args[1])?,
				max_supply,
				mint_settings,
			]))
		},
		Extrinsic::MintNFT => {
			println!("{:?}", raw_args.len());
			if raw_args.len() < 4 {
				return Err(Error::ParsingArgsError);
			}
			args.push(parse_u128(&raw_args[0])?);
			args.push(parse_u128(&raw_args[1])?);
			args.push(parse_account_id(&raw_args[2])?);
			if raw_args[3] == "None" && raw_args.len() == 4 {
				args.push(Value::unnamed_variant("None", vec![]));
			} else {
				if raw_args.len() < 5 {
					return Err(Error::ParsingArgsError);
				}
				let owned_item = parse_optional_u128(&raw_args[3])?;
				let mint_price = parse_optional_u128(&raw_args[4])?;
				args.push(Value::unnamed_variant(
					"Some",
					vec![Value::unnamed_composite(vec![owned_item, mint_price])],
				));
			}
		},
		Extrinsic::Transfer => {
			if raw_args.len() < 2 {
				return Err(Error::ParsingArgsError);
			}
			args.push(parse_account_id(&raw_args[0])?);
			args.push(parse_u128(&raw_args[1])?);
		},
	}
	Ok(args)
}

// Helper function to parse u128
fn parse_u128(arg: &str) -> Result<Value, Error> {
	Ok(Value::u128(arg.parse::<u128>().map_err(|_| Error::ParsingArgsError)?))
}
// Helper function to parse account id
fn parse_account_id(arg: &str) -> Result<Value, Error> {
	Ok(Value::unnamed_variant("Id", vec![Value::from_bytes(parse_account(arg)?)]))
}
// Helper function to handle "None" or Some(u128) values
fn parse_optional_u128(arg: &str) -> Result<Value, Error> {
	if arg == "None" {
		Ok(Value::unnamed_variant("None", vec![]))
	} else {
		Ok(Value::unnamed_variant("Some", vec![parse_u128(arg)?]))
	}
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
