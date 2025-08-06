// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use params::Param;
use scale_value::stringify::custom_parsers;
use std::fmt::{Display, Formatter};
use subxt::{dynamic::Value, utils::to_hex, Metadata, OnlineClient, SubstrateConfig};

pub mod action;
pub mod params;

/// Represents a pallet in the blockchain, including its dispatchable functions.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Pallet {
	/// The name of the pallet.
	pub name: String,
	/// The index of the pallet within the runtime.
	pub index: u8,
	/// The documentation of the pallet.
	pub docs: String,
	/// The dispatchable functions of the pallet.
	pub functions: Vec<Function>,
}

impl Display for Pallet {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.name)
	}
}

/// Represents a dispatchable function.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Function {
	/// The pallet containing the dispatchable function.
	pub pallet: String,
	/// The name of the function.
	pub name: String,
	/// The index of the function within the pallet.
	pub index: u8,
	/// The documentation of the function.
	pub docs: String,
	/// The parameters of the function.
	pub params: Vec<Param>,
	/// Whether this function is supported (no recursive or unsupported types like `RuntimeCall`).
	pub is_supported: bool,
}

impl Display for Function {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.name)
	}
}

/// Parses the chain metadata to extract information about pallets and their dispatchable functions.
///
/// # Arguments
/// * `client`: The client to interact with the chain.
///
/// NOTE: pallets are ordered by their index within the runtime by default.
pub fn parse_chain_metadata(client: &OnlineClient<SubstrateConfig>) -> Result<Vec<Pallet>, Error> {
	let metadata: Metadata = client.metadata();

	let pallets = metadata
		.pallets()
		.map(|pallet| {
			let functions = pallet
				.call_variants()
				.map(|variants| {
					variants
						.iter()
						.map(|variant| {
							let mut is_supported = true;

							// Parse parameters for the dispatchable function.
							let params = {
								let mut parsed_params = Vec::new();
								for field in &variant.fields {
									match params::field_to_param(&metadata, field) {
										Ok(param) => parsed_params.push(param),
										Err(_) => {
											// If an error occurs while parsing the values, mark the
											// dispatchable function as unsupported rather than
											// error.
											is_supported = false;
											break;
										},
									}
								}
								parsed_params
							};

							Ok(Function {
								pallet: pallet.name().to_string(),
								name: variant.name.clone(),
								index: variant.index,
								docs: if is_supported {
									// Filter out blank lines and then flatten into a single value.
									variant
										.docs
										.iter()
										.filter(|l| !l.is_empty())
										.cloned()
										.collect::<Vec<_>>()
										.join(" ")
								} else {
									// To display the message in the UI
									"Function Not Supported".to_string()
								},
								params,
								is_supported,
							})
						})
						.collect::<Result<Vec<Function>, Error>>()
				})
				.unwrap_or_else(|| Ok(vec![]))?;

			Ok(Pallet {
				name: pallet.name().to_string(),
				index: pallet.index(),
				docs: pallet.docs().join(" "),
				functions,
			})
		})
		.collect::<Result<Vec<Pallet>, Error>>()?;

	Ok(pallets)
}

/// Finds a specific pallet by name and retrieves its details from metadata.
///
/// # Arguments
/// * `pallets`: List of pallets available within the chain's runtime.
/// * `pallet_name`: The name of the pallet to find.
pub fn find_pallet_by_name<'a>(
	pallets: &'a [Pallet],
	pallet_name: &str,
) -> Result<&'a Pallet, Error> {
	if let Some(pallet) = pallets.iter().find(|p| p.name == pallet_name) {
		Ok(pallet)
	} else {
		Err(Error::PalletNotFound(pallet_name.to_string()))
	}
}

/// Finds a specific dispatchable function by name and retrieves its details from metadata.
///
/// # Arguments
/// * `pallets`: List of pallets available within the chain's runtime.
/// * `pallet_name`: The name of the pallet.
/// * `function_name`: Name of the dispatchable function to locate.
pub fn find_dispatchable_by_name<'a>(
	pallets: &'a [Pallet],
	pallet_name: &str,
	function_name: &str,
) -> Result<&'a Function, Error> {
	let pallet = find_pallet_by_name(pallets, pallet_name)?;
	if let Some(function) = pallet.functions.iter().find(|&e| e.name == function_name) {
		Ok(function)
	} else {
		Err(Error::FunctionNotSupported)
	}
}

/// Parses and processes raw string parameter values for a dispatchable function, mapping them to
/// `Value` types.
///
/// # Arguments
/// * `params`: The metadata definition for each parameter of the corresponding dispatchable
///   function.
/// * `raw_params`: A vector of raw string arguments for the dispatchable function.
pub fn parse_dispatchable_arguments(
	params: &[Param],
	raw_params: Vec<String>,
) -> Result<Vec<Value>, Error> {
	params
		.iter()
		.zip(raw_params)
		.map(|(param, raw_param)| {
			// Convert sequence parameters to hex if is_sequence
			let processed_param = if param.is_sequence && !raw_param.starts_with("0x") {
				to_hex(&raw_param)
			} else {
				raw_param
			};
			scale_value::stringify::from_str_custom()
				.add_custom_parser(custom_parsers::parse_hex)
				.add_custom_parser(custom_parsers::parse_ss58)
				.parse(&processed_param)
				.0
				.map_err(|_| Error::ParamProcessingError)
		})
		.collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use sp_core::bytes::from_hex;
	use subxt::ext::scale_bits;

	#[test]
	fn parse_dispatchable_arguments_works() -> Result<()> {
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
			from_hex("8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48")
				.unwrap()
				.into_iter()
				.map(|b| Value::u128(b as u128))
				.collect();
		// Define mock dispatchable function parameters for testing.
		let params = vec![
			Param { type_name: "u128".to_string(), ..Default::default() },
			Param { type_name: "i128".to_string(), ..Default::default() },
			Param { type_name: "bool".to_string(), ..Default::default() },
			Param { type_name: "char".to_string(), ..Default::default() },
			Param { type_name: "string".to_string(), ..Default::default() },
			Param { type_name: "compostie".to_string(), ..Default::default() },
			Param { type_name: "variant".to_string(), is_variant: true, ..Default::default() },
			Param { type_name: "bit_sequence".to_string(), ..Default::default() },
			Param { type_name: "tuple".to_string(), is_tuple: true, ..Default::default() },
			Param { type_name: "composite".to_string(), ..Default::default() },
		];
		assert_eq!(
			parse_dispatchable_arguments(&params, args)?,
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
