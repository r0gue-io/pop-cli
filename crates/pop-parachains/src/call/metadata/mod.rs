// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use params::Param;
use scale_value::stringify::custom_parsers;
use std::fmt::{Display, Formatter};
use subxt::{dynamic::Value, Metadata, OnlineClient, SubstrateConfig};

pub mod action;
pub mod params;

/// Represents a pallet in the blockchain, including its extrinsics.
#[derive(Clone, PartialEq, Eq)]
pub struct Pallet {
	/// The name of the pallet.
	pub name: String,
	/// The documentation of the pallet.
	pub docs: String,
	/// The extrinsics of the pallet.
	pub extrinsics: Vec<Extrinsic>,
}

impl Display for Pallet {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.name)
	}
}

/// Represents an extrinsic in a pallet.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Extrinsic {
	/// The name of the extrinsic.
	pub name: String,
	/// The documentation of the extrinsic.
	pub docs: String,
	/// The parameters of the extrinsic.
	pub params: Vec<Param>,
	/// Whether this extrinsic is supported (no recursive or unsupported types like `RuntimeCall`).
	pub is_supported: bool,
}

impl Display for Extrinsic {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.name)
	}
}

/// Parses the chain metadata to extract information about pallets and their extrinsics with its
/// parameters.
///
/// # Arguments
/// * `api`: Reference to an `OnlineClient` connected to the chain.
pub async fn parse_chain_metadata(
	api: &OnlineClient<SubstrateConfig>,
) -> Result<Vec<Pallet>, Error> {
	let metadata: Metadata = api.metadata();

	let pallets = metadata
		.pallets()
		.map(|pallet| {
			let extrinsics = pallet
				.call_variants()
				.map(|variants| {
					variants
						.iter()
						.map(|variant| {
							let mut is_supported = true;

							// Parse parameters for the extrinsic
							let params = {
								let mut parsed_params = Vec::new();
								for field in &variant.fields {
									match params::field_to_param(api, field) {
										Ok(param) => parsed_params.push(param),
										Err(_) => {
											// If an error occurs while parsing the values, mark the
											// extrinsic as unsupported rather than error.
											is_supported = false;
											parsed_params.clear();
											break;
										},
									}
								}
								parsed_params
							};

							Ok(Extrinsic {
								name: variant.name.clone(),
								docs: if is_supported {
									variant.docs.concat()
								} else {
									// To display the message in the UI
									"Extrinsic Not Supported".to_string()
								},
								params,
								is_supported,
							})
						})
						.collect::<Result<Vec<Extrinsic>, Error>>()
				})
				.unwrap_or_else(|| Ok(vec![]))?;

			Ok(Pallet {
				name: pallet.name().to_string(),
				docs: pallet.docs().join(" "),
				extrinsics,
			})
		})
		.collect::<Result<Vec<Pallet>, Error>>()?;

	Ok(pallets)
}

/// Finds a specific pallet by name and retrieves its details from metadata.
///
/// # Arguments
/// * `pallets`: List of pallets availables in the chain.
/// * `pallet_name`: The name of the pallet to find.
pub async fn find_pallet_by_name(pallets: &[Pallet], pallet_name: &str) -> Result<Pallet, Error> {
	if let Some(pallet) = pallets.iter().find(|p| p.name == pallet_name) {
		Ok(pallet.clone())
	} else {
		Err(Error::PalletNotFound(pallet_name.to_string()))
	}
}

/// Finds a specific extrinsic by name and retrieves its details from metadata.
///
/// # Arguments
/// * `pallets`: List of pallets availables in the chain.
/// * `pallet_name`: The name of the pallet to find.
/// * `extrinsic_name`: Name of the extrinsic to locate.
pub async fn find_extrinsic_by_name(
	pallets: &[Pallet],
	pallet_name: &str,
	extrinsic_name: &str,
) -> Result<Extrinsic, Error> {
	let pallet = find_pallet_by_name(pallets, pallet_name).await?;
	if let Some(extrinsic) = pallet.extrinsics.iter().find(|&e| e.name == extrinsic_name) {
		Ok(extrinsic.clone())
	} else {
		Err(Error::ExtrinsicNotSupported)
	}
}

/// Parses and processes raw string parameters for an extrinsic, mapping them to `Value` types.
///
/// # Arguments
/// * `raw_params`: A vector of raw string arguments for the extrinsic.
pub async fn parse_extrinsic_arguments(raw_params: Vec<String>) -> Result<Vec<Value>, Error> {
	let mut parsed_params: Vec<Value> = Vec::new();
	for raw_param in raw_params {
		let parsed_value: Value = scale_value::stringify::from_str_custom()
			.add_custom_parser(custom_parsers::parse_hex)
			.add_custom_parser(custom_parsers::parse_ss58)
			.parse(&raw_param)
			.0
			.map_err(|_| Error::ParamProcessingError)?;
		parsed_params.push(parsed_value);
	}
	Ok(parsed_params)
}

#[cfg(test)]
mod tests {
	use super::*;

	use crate::set_up_api;
	use anyhow::Result;
	use subxt::ext::scale_bits;

	#[tokio::test]
	async fn parse_chain_metadata_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let pallets = parse_chain_metadata(&api).await?;
		// Test the first pallet is parsed correctly
		let first_pallet = pallets.first().unwrap();
		assert_eq!(first_pallet.name, "System");
		assert_eq!(first_pallet.docs, "");
		assert_eq!(first_pallet.extrinsics.len(), 11);
		let first_extrinsic = first_pallet.extrinsics.first().unwrap();
		assert_eq!(first_extrinsic.name, "remark");
		assert_eq!(
			first_extrinsic.docs,
			"Make some on-chain remark.Can be executed by every `origin`."
		);
		assert!(first_extrinsic.is_supported);
		assert_eq!(first_extrinsic.params.first().unwrap().name, "remark");
		assert_eq!(first_extrinsic.params.first().unwrap().type_name, "[u8]");
		assert_eq!(first_extrinsic.params.first().unwrap().sub_params.len(), 0);
		assert!(!first_extrinsic.params.first().unwrap().is_optional);
		assert!(!first_extrinsic.params.first().unwrap().is_tuple);
		assert!(!first_extrinsic.params.first().unwrap().is_variant);
		Ok(())
	}

	#[tokio::test]
	async fn find_pallet_by_name_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let pallets = parse_chain_metadata(&api).await?;
		assert!(matches!(
			find_pallet_by_name(&pallets, "WrongName").await,
			Err(Error::PalletNotFound(pallet)) if pallet == "WrongName".to_string()));
		let pallet = find_pallet_by_name(&pallets, "Balances").await?;
		assert_eq!(pallet.name, "Balances");
		assert_eq!(pallet.extrinsics.len(), 9);
		Ok(())
	}

	#[tokio::test]
	async fn find_extrinsic_by_name_works() -> Result<()> {
		let api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let pallets = parse_chain_metadata(&api).await?;
		assert!(matches!(
			find_extrinsic_by_name(&pallets, "WrongName", "wrong_extrinsic").await,
			Err(Error::PalletNotFound(pallet)) if pallet == "WrongName".to_string()));
		assert!(matches!(
			find_extrinsic_by_name(&pallets, "Balances", "wrong_extrinsic").await,
			Err(Error::ExtrinsicNotSupported)
		));
		let extrinsic = find_extrinsic_by_name(&pallets, "Balances", "force_transfer").await?;
		assert_eq!(extrinsic.name, "force_transfer");
		assert_eq!(extrinsic.docs, "Exactly as `transfer_allow_death`, except the origin must be root and the source accountmay be specified.");
		assert_eq!(extrinsic.is_supported, true);
		assert_eq!(extrinsic.params.len(), 3);
		Ok(())
	}

	#[tokio::test]
	async fn parse_extrinsic_arguments_works() -> Result<()> {
		// Values for testing from: https://docs.rs/scale-value/0.18.0/scale_value/stringify/fn.from_str.html
		// and https://docs.rs/scale-value/0.18.0/scale_value/stringify/fn.from_str_custom.html
		let args = [
			"1".to_string(),
			"-1".to_string(),
			"true".to_string(),
			"'a'".to_string(),
			"\"hi\"".to_string(),
			"{ a: true, b: \"hello\" }".to_string(),
			"MyVariant { a: true, b: \"hello\" }".to_string(),
			"<0101>".to_string(),
			"(1,2,0x030405)".to_string(),
			r#"{
				name: "Alice",
				address: 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty
			}"#
			.to_string(),
		]
		.to_vec();
		let addr: Vec<_> =
			hex::decode("8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48")
				.unwrap()
				.into_iter()
				.map(|b| Value::u128(b as u128))
				.collect();
		assert_eq!(
			parse_extrinsic_arguments(args).await?,
			[
				Value::u128(1),
				Value::i128(-1),
				Value::bool(true),
				Value::char('a'),
				Value::string("hi"),
				Value::named_composite(vec![
					("a", Value::bool(true)),
					("b", Value::string("hello"))
				]),
				Value::named_variant(
					"MyVariant",
					vec![("a", Value::bool(true)), ("b", Value::string("hello"))]
				),
				Value::bit_sequence(scale_bits::Bits::from_iter([false, true, false, true])),
				Value::unnamed_composite(vec![
					Value::u128(1),
					Value::u128(2),
					Value::unnamed_composite(vec![Value::u128(3), Value::u128(4), Value::u128(5),])
				]),
				Value::named_composite(vec![
					("name", Value::string("Alice")),
					("address", Value::unnamed_composite(addr))
				])
			]
		);
		Ok(())
	}
}
