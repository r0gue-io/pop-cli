// SPDX-License-Identifier: GPL-3.0

use crate::cli::{
	self,
	traits::{Confirm as _, Input as _},
};
use anyhow::Result;
use pop_common::parse_account;
use pop_parachains::{parse_string_into_scale_value, Extrinsic, Pallet, Value};

/// Prompt the user to select an operation.
pub fn prompt_arguments(
	extrinsic: &Extrinsic,
	cli: &mut impl cli::traits::Cli,
) -> Result<Vec<Value>> {
	match extrinsic {
		Extrinsic::CreateAsset => prompt_query_params(&extrinsic, cli),
		Extrinsic::MintAsset => prompt_query_params(&extrinsic, cli),
		Extrinsic::CreateCollection => prompt_query_params(&extrinsic, cli),
		Extrinsic::MintNFT => prompt_query_params(&extrinsic, cli),
		Extrinsic::Transfer => prompt_query_params(&extrinsic, cli),
	}
}
fn prompt_query_params(
	extrinsic: &Extrinsic,
	cli: &mut impl cli::traits::Cli,
) -> Result<Vec<Value>> {
	let mut args: Vec<Value> = Vec::new();
	let id = cli
		.input(&format!(
			"Enter the {} id",
			if extrinsic.pallet()? == Pallet::Assets.to_string() { "Asset" } else { "Collection" }
		))
		.placeholder("0")
		.required(false)
		.interact()?;
	args.push(parse_string_into_scale_value(&id)?);
	// if call == &Call::NFTItem {
	// 	let nft_id = cli
	// 		.input("Enter the Nft id")
	// 		.placeholder("0 or None")
	// 		.required(false)
	// 		.interact()?;
	// 	args.push(parse_string_into_scale_value(&nft_id)?);
	// }
	Ok(args)
}
fn prompt_mint_params(
	extrinsic: &Extrinsic,
	cli: &mut impl cli::traits::Cli,
) -> Result<Vec<Value>> {
	let mut args: Vec<Value> = Vec::new();
	let id = cli
		.input(&format!(
			"Enter the {} id",
			if extrinsic.pallet()? == Pallet::Assets.to_string() { "Asset" } else { "Collection" }
		))
		.placeholder("0")
		.required(true)
		.interact()?;
	args.push(parse_string_into_scale_value(&id)?);
	if extrinsic == &Extrinsic::MintNFT {
		let nft_id = cli
			.input(&format!(
				"Enter the {} id",
				if extrinsic.pallet()? == Pallet::Assets.to_string() {
					"Asset"
				} else {
					"Collection"
				}
			))
			.placeholder("0")
			.required(true)
			.interact()?;
		args.push(parse_string_into_scale_value(&nft_id)?);
	}
	// Prompt for beneficiary
	let beneficiary: String = cli
		.input("Enter the beneficiary address")
		.placeholder("e.g. 5DYs7UGBm2LuX4ryvyqfksozNAW5V47tPbGiVgnjYWCZ29bt")
		.required(true)
		.validate(|input: &String| match parse_account(input) {
			Ok(_) => Ok(()),
			Err(_) => Err("Invalid address."),
		})
		.interact()?;
	args.push(parse_string_into_scale_value(&beneficiary)?);
	// if extrinsic == &Extrinsic::AssetItem {
	// 	let amount = cli.input("Enter the amount").placeholder("0").required(true).interact()?;
	// 	args.push(parse_string_into_scale_value(&amount)?);
	// }
	if extrinsic == &Extrinsic::MintNFT {
		if cli
			.confirm("Do you want to include witness data?")
			.initial_value(false)
			.interact()?
		{
			let config = {};
			let owned_item = cli
				.input("Id of the item in a required collection:")
				.placeholder("0 or None")
				.required(false)
				.interact()?;
			let owned_item = cli
				.input("The price specified in mint settings:")
				.placeholder("0 or None")
				.required(false)
				.interact()?;
			//args.push(Value::u128(id));
		}
	}
	Ok(args)
}
