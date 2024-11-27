// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use params::Param;
use scale_value::stringify::custom_parsers;
use std::fmt::{Display, Formatter};
use subxt::{dynamic::Value, Metadata, OnlineClient, SubstrateConfig};

pub mod action;
pub mod params;

#[derive(Clone, PartialEq, Eq)]
/// Represents a pallet in the blockchain, including its extrinsics.
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

#[derive(Clone, PartialEq, Eq, Debug)]
/// Represents an extrinsic in a pallet.
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
									match params::field_to_param(api, &variant.name, field) {
										Ok(param) => parsed_params.push(param),
										Err(Error::ExtrinsicNotSupported(_)) => {
											// Unsupported extrinsic due to complex types
											is_supported = false;
											parsed_params.clear();
											break;
										},
										Err(e) => return Err(e),
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
		return Ok(extrinsic.clone());
	} else {
		return Err(Error::ExtrinsicNotSupported(extrinsic_name.to_string()));
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
