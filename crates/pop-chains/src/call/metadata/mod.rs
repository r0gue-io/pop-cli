// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use params::Param;
use scale_value::{Composite, ValueDef, stringify::custom_parsers};
use std::fmt::{Display, Formatter, Write};
use subxt::{
	Metadata, OnlineClient, SubstrateConfig,
	dynamic::Value,
	ext::futures::TryStreamExt,
	metadata::types::{PalletMetadata, StorageEntryType},
	utils::to_hex,
};

pub mod action;
pub mod params;

pub type RawValue = Value<u32>;

fn format_single_tuples<T, W: Write>(value: &Value<T>, mut writer: W) -> Option<core::fmt::Result> {
	if let ValueDef::Composite(Composite::Unnamed(vals)) = &value.value &&
		vals.len() == 1
	{
		let val = &vals[0];
		return match raw_value_to_string(val, "") {
			Ok(r) => match writer.write_str(&r) {
				Ok(_) => Some(Ok(())),
				Err(_) => None,
			},
			Err(_) => None,
		};
	}
	None
}

// Formats to hexadecimal in lowercase
fn format_hex<T, W: Write>(value: &Value<T>, mut writer: W) -> Option<core::fmt::Result> {
	let mut result = String::new();
	match scale_value::stringify::custom_formatters::format_hex(value, &mut result) {
		Some(res) => match res {
			Ok(_) => match writer.write_str(&result.to_lowercase()) {
				Ok(_) => Some(Ok(())),
				Err(_) => None,
			},
			Err(_) => None,
		},
		None => None,
	}
}

/// Converts a raw SCALE value to a human-readable string representation.
///
/// This function takes a raw SCALE value and formats it into a string using custom formatters:
/// - Formats byte sequences as hex strings.
/// - Unwraps single-element tuples.
/// - Uses pretty printing for better readability.
///
/// # Arguments
/// * `value` - The raw SCALE value to convert to string.
///
/// # Returns
/// * `Ok(String)` - The formatted string representation of the value.
/// * `Err(_)` - If the value cannot be converted to string.
pub fn raw_value_to_string<T>(value: &Value<T>, indent: &str) -> anyhow::Result<String> {
	let mut result = String::new();
	scale_value::stringify::to_writer_custom()
		.compact()
		.pretty()
		.add_custom_formatter(|v, w| format_hex(v, w))
		.add_custom_formatter(|v, w| format_single_tuples(v, w))
		.write(value, &mut result)?;

	// Add indentation to each line
	let indented = result
		.lines()
		.map(|line| format!("{indent}{line}"))
		.collect::<Vec<_>>()
		.join("\n");
	Ok(indented)
}

/// Renders storage key-value pairs into a human-readable string format.
///
/// Takes a slice of tuples containing storage keys and their associated values and formats them
/// into a readable string representation. Each key-value pair is rendered on separate lines within
/// square brackets.
///
/// # Arguments
/// * `key_value_pairs` - A slice of tuples where each tuple contains:
///   - A vector of storage keys.
///   - The associated storage value.
///
/// # Returns
/// * `Ok(String)` - A formatted string containing the rendered key-value pairs.
/// * `Err(_)` - If there's an error converting the values to strings.
pub fn render_storage_key_values(
	key_value_pairs: &[(Vec<Value>, RawValue)],
) -> anyhow::Result<String> {
	let mut result = String::new();
	let indent = "  ";
	for (keys, value) in key_value_pairs {
		result.push_str("[\n");
		if !keys.is_empty() {
			for key in keys {
				result.push_str(&raw_value_to_string(key, indent)?);
				result.push_str(",\n");
			}
		}
		result.push_str(&raw_value_to_string(value, indent)?);
		result.push_str("\n]\n");
	}
	Ok(result)
}

/// Represents different types of callable items that can be interacted with in the runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CallItem {
	/// A dispatchable function (extrinsic) that can be called.
	Function(Function),
	/// A constant value defined in the runtime.
	Constant(Constant),
	/// A storage item that can be queried.
	Storage(Storage),
}

impl Default for CallItem {
	fn default() -> Self {
		Self::Function(Function::default())
	}
}

impl Display for CallItem {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			CallItem::Function(function) => function.fmt(f),
			CallItem::Constant(constant) => constant.fmt(f),
			CallItem::Storage(storage) => storage.fmt(f),
		}
	}
}

impl CallItem {
	/// Returns a reference to the [`Function`] if this is a function call item.
	pub fn as_function(&self) -> Option<&Function> {
		match self {
			CallItem::Function(f) => Some(f),
			_ => None,
		}
	}

	/// Returns a reference to the [`Constant`] if this is a constant call item.
	pub fn as_constant(&self) -> Option<&Constant> {
		match self {
			CallItem::Constant(c) => Some(c),
			_ => None,
		}
	}

	/// Returns a reference to the [`Storage`] if this is a storage call item.
	pub fn as_storage(&self) -> Option<&Storage> {
		match self {
			CallItem::Storage(s) => Some(s),
			_ => None,
		}
	}

	/// Returns the name of this call item.
	pub fn name(&self) -> &str {
		match self {
			CallItem::Function(function) => &function.name,
			CallItem::Constant(constant) => &constant.name,
			CallItem::Storage(storage) => &storage.name,
		}
	}
	/// Returns a descriptive hint string indicating the type of this call item.
	pub fn hint(&self) -> &str {
		match self {
			CallItem::Function(_) => "📝 [EXTRINSIC]",
			CallItem::Constant(_) => "[CONSTANT]",
			CallItem::Storage(_) => "[STORAGE]",
		}
	}

	/// Returns the documentation string associated with this call item.
	pub fn docs(&self) -> &str {
		match self {
			CallItem::Function(function) => &function.docs,
			CallItem::Constant(constant) => &constant.docs,
			CallItem::Storage(storage) => &storage.docs,
		}
	}

	/// Returns the name of the pallet containing this call item.
	pub fn pallet(&self) -> &str {
		match self {
			CallItem::Function(function) => &function.pallet,
			CallItem::Constant(constant) => &constant.pallet,
			CallItem::Storage(storage) => &storage.pallet,
		}
	}
}

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
	/// The constants of the pallet.
	pub constants: Vec<Constant>,
	/// The storage items of the pallet.
	pub state: Vec<Storage>,
}

impl Display for Pallet {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.name)
	}
}

impl Pallet {
	/// Returns a vector containing all callable items (functions, constants, and storage) defined
	/// in this pallet.
	///
	/// This method collects and returns all available callable items from the pallet:
	/// - Dispatchable functions (extrinsics)
	/// - Constants
	/// - Storage items
	///
	/// # Returns
	/// A `Vec<CallItem>` containing all callable items from this pallet.
	pub fn get_all_callables(&self) -> Vec<CallItem> {
		let mut callables = Vec::new();
		for function in &self.functions {
			callables.push(CallItem::Function(function.clone()));
		}
		for constant in &self.constants {
			callables.push(CallItem::Constant(constant.clone()));
		}
		for storage in &self.state {
			callables.push(CallItem::Storage(storage.clone()));
		}
		callables
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

/// Represents a runtime constant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constant {
	/// The pallet containing the dispatchable function.
	pub pallet: String,
	/// The name of the constant.
	pub name: String,
	/// The documentation of the constant.
	pub docs: String,
	/// The value of the constant.
	pub value: RawValue,
}

impl Display for Constant {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.name)
	}
}

/// Represents a storage item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Storage {
	/// The pallet containing the storage item.
	pub pallet: String,
	/// The name of the storage item.
	pub name: String,
	/// The documentation of the storage item.
	pub docs: String,
	/// The type ID for decoding the storage value.
	pub type_id: u32,
	/// Optional type ID for map-type storage items. Usually a tuple.
	pub key_id: Option<u32>,
	/// Whether to query all values for a storage item, optionally filtered by provided keys.
	pub query_all: bool,
}

impl Display for Storage {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.name)
	}
}

impl Storage {
	/// Queries all values for a storage item, optionally filtered by provided keys.
	///
	/// This method allows retrieving multiple values from storage by iterating through all entries
	/// that match the provided keys. For map-type storage items, keys can be used to filter
	/// the results.
	///
	/// # Arguments
	/// * `client` - The client to interact with the chain.
	/// * `keys` - Optional storage keys for map-type storage items to filter results.
	pub async fn query_all(
		&self,
		client: &OnlineClient<SubstrateConfig>,
		keys: Vec<Value>,
	) -> Result<Vec<(Vec<Value>, RawValue)>, Error> {
		let mut elements = Vec::new();
		let metadata = client.metadata();
		let types = metadata.types();
		let storage_address = subxt::dynamic::storage(&self.pallet, &self.name, keys);
		let mut stream = client
			.storage()
			.at_latest()
			.await
			.map_err(|e| Error::MetadataParsingError(format!("Failed to get storage: {}", e)))?
			.iter(storage_address)
			.await
			.map_err(|e| {
				Error::MetadataParsingError(format!("Failed to fetch storage value: {}", e))
			})?;

		while let Some(storage_data) = stream.try_next().await.map_err(|e| {
			Error::MetadataParsingError(format!("Failed to fetch storage value: {}", e))
		})? {
			let keys = storage_data.keys;
			let mut bytes = storage_data.value.encoded();
			let decoded_value = scale_value::scale::decode_as_type(&mut bytes, self.type_id, types)
				.map_err(|e| {
					Error::MetadataParsingError(format!("Failed to decode storage value: {}", e))
				})?;
			elements.push((keys, decoded_value));
		}
		Ok(elements)
	}
	/// Query the storage value from the chain and return it as a formatted string.
	///
	/// # Arguments
	/// * `client` - The client to interact with the chain.
	/// * `keys` - Optional storage keys for map-type storage items.
	pub async fn query(
		&self,
		client: &OnlineClient<SubstrateConfig>,
		keys: Vec<Value>,
	) -> Result<Option<RawValue>, Error> {
		let metadata = client.metadata();
		let types = metadata.types();
		let storage_address = subxt::dynamic::storage(&self.pallet, &self.name, keys);
		let storage_data = client
			.storage()
			.at_latest()
			.await
			.map_err(|e| Error::MetadataParsingError(format!("Failed to get storage: {}", e)))?
			.fetch(&storage_address)
			.await
			.map_err(|e| {
				Error::MetadataParsingError(format!("Failed to fetch storage value: {}", e))
			})?;

		// Decode the value if it exists
		match storage_data {
			Some(value) => {
				// Try to decode using the type information
				let mut bytes = value.encoded();
				let decoded_value = scale_value::scale::decode_as_type(
					&mut bytes,
					self.type_id,
					types,
				)
				.map_err(|e| {
					Error::MetadataParsingError(format!("Failed to decode storage value: {}", e))
				})?;

				Ok(Some(decoded_value))
			},
			None => Ok(None),
		}
	}
}

fn extract_chain_state_from_pallet_metadata(
	pallet: &PalletMetadata,
) -> anyhow::Result<Vec<Storage>> {
	pallet
		.storage()
		.map(|storage_metadata| {
			storage_metadata
				.entries()
				.iter()
				.map(|entry| {
					Ok(Storage {
						pallet: pallet.name().to_string(),
						name: entry.name().to_string(),
						docs: entry
							.docs()
							.iter()
							.filter(|l| !l.is_empty())
							.cloned()
							.collect::<Vec<_>>()
							.join("")
							.trim()
							.to_string(),
						type_id: entry.entry_type().value_ty(),
						key_id: match entry.entry_type() {
							StorageEntryType::Plain(_) => None,
							StorageEntryType::Map { key_ty, .. } => Some(*key_ty),
						},
						query_all: false,
					})
				})
				.collect::<Result<Vec<Storage>, Error>>()
		})
		.unwrap_or_else(|| Ok(vec![]))
		.map_err(|e| anyhow::Error::msg(e.to_string()))
}

fn extract_constants_from_pallet_metadata(
	pallet: &PalletMetadata,
	metadata: &Metadata,
) -> anyhow::Result<Vec<Constant>> {
	let types = metadata.types();
	pallet
		.constants()
		.map(|constant| {
			// Decode the SCALE-encoded constant value using its type information
			let mut value_bytes = constant.value();
			let decoded_value =
				scale_value::scale::decode_as_type(&mut value_bytes, constant.ty(), types)
					.map_err(|e| {
						Error::MetadataParsingError(format!(
							"Failed to decode constant {}: {}",
							constant.name(),
							e
						))
					})?;

			Ok(Constant {
				pallet: pallet.name().to_string(),
				name: constant.name().to_string(),
				docs: constant
					.docs()
					.iter()
					.filter(|l| !l.is_empty())
					.cloned()
					.collect::<Vec<_>>()
					.join("")
					.trim()
					.to_string(),
				value: decoded_value,
			})
		})
		.collect::<Result<Vec<Constant>, Error>>()
		.map_err(|e| anyhow::Error::msg(e.to_string()))
}

fn extract_functions_from_pallet_metadata(
	pallet: &PalletMetadata,
	metadata: &Metadata,
) -> anyhow::Result<Vec<Function>> {
	pallet
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
							match params::field_to_param(metadata, field) {
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
								.trim()
								.to_string()
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
		.unwrap_or_else(|| Ok(vec![]))
		.map_err(|e| anyhow::Error::msg(e.to_string()))
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
			Ok(Pallet {
				name: pallet.name().to_string(),
				index: pallet.index(),
				docs: pallet.docs().join("").trim().to_string(),
				functions: extract_functions_from_pallet_metadata(&pallet, &metadata)?,
				constants: extract_constants_from_pallet_metadata(&pallet, &metadata)?,
				state: extract_chain_state_from_pallet_metadata(&pallet)?,
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
pub fn find_callable_by_name(
	pallets: &[Pallet],
	pallet_name: &str,
	function_name: &str,
) -> Result<CallItem, Error> {
	let pallet = find_pallet_by_name(pallets, pallet_name)?;
	if let Some(function) = pallet.functions.iter().find(|&e| e.name == function_name) {
		return Ok(CallItem::Function(function.clone()));
	}
	if let Some(constant) = pallet.constants.iter().find(|&e| e.name == function_name) {
		return Ok(CallItem::Constant(constant.clone()));
	}
	if let Some(storage) = pallet.state.iter().find(|&e| e.name == function_name) {
		return Ok(CallItem::Storage(storage.clone()));
	}
	Err(Error::FunctionNotFound(format!(
		"Could not find a function, constant or storage with the name \"{function_name}\""
	)))
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
			let processed_param = if param.is_sequence && !raw_param.starts_with("0x") {
				if param.type_name == "[u8]" {
					// Convert byte sequence parameters to hex
					to_hex(&raw_param)
				} else {
					// For other sequences (e.g., Vec<AccountId32>), convert bracket syntax
					// [a, b, c] to parentheses (a, b, c) since scale_value uses parentheses
					// for unnamed composites. This allows SS58 parsing inside arrays.
					convert_brackets_to_parens(&raw_param)
				}
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

/// Converts bracket array syntax `[a, b, c]` to parentheses `(a, b, c)` for scale_value parsing.
/// Only converts outermost brackets when the string starts with `[` and ends with `]`.
fn convert_brackets_to_parens(input: &str) -> String {
	let trimmed = input.trim();
	if trimmed.starts_with('[') && trimmed.ends_with(']') {
		format!("({})", &trimmed[1..trimmed.len() - 1])
	} else {
		input.to_string()
	}
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
			from_hex("8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48")?
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
			Param { type_name: "composite".to_string(), ..Default::default() },
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

	#[test]
	fn parse_vec_account_id() -> Result<()> {
		// Test case from issue #906: Vec<AccountId32> should parse SS58 addresses
		let params = vec![Param {
			name: "who".into(),
			type_name: "[AccountId32 ([u8;32])]".into(),
			is_sequence: true,
			..Default::default()
		}];
		let args = vec!["[5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty]".into()];
		let result = parse_dispatchable_arguments(&params, args);
		assert!(result.is_ok(), "Failed to parse: {:?}", result);

		// Verify the parsed value is a composite containing the decoded SS58 address
		let values = result?;
		assert_eq!(values.len(), 1);

		// The expected AccountId bytes for 5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty
		let addr: Vec<_> =
			from_hex("8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48")?
				.into_iter()
				.map(|b| Value::u128(b as u128))
				.collect();

		assert_eq!(values[0], Value::unnamed_composite(vec![Value::unnamed_composite(addr)]));
		Ok(())
	}

	#[test]
	fn parse_vec_multiple_account_ids() -> Result<()> {
		// Test multiple AccountIds in a Vec
		let params = vec![Param {
			name: "who".into(),
			type_name: "[AccountId32 ([u8;32])]".into(),
			is_sequence: true,
			..Default::default()
		}];
		let args = vec![
			"[5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty, 5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY]".into(),
		];
		let result = parse_dispatchable_arguments(&params, args);
		assert!(result.is_ok());

		let values = result?;
		assert_eq!(values.len(), 1);

		// Both addresses should be parsed
		if let ValueDef::Composite(composite) = &values[0].value {
			match composite {
				Composite::Unnamed(items) => assert_eq!(items.len(), 2),
				_ => panic!("Expected unnamed composite"),
			}
		} else {
			panic!("Expected composite value");
		}
		Ok(())
	}

	#[test]
	fn parse_byte_sequence_still_works() -> Result<()> {
		// Ensure [u8] sequences still work (regression test)
		let params = vec![Param {
			name: "remark".into(),
			type_name: "[u8]".into(),
			is_sequence: true,
			..Default::default()
		}];
		let args = vec!["hello".into()];
		let result = parse_dispatchable_arguments(&params, args);
		assert!(result.is_ok());
		Ok(())
	}

	#[test]
	fn convert_brackets_to_parens_works() {
		// Standard array conversion
		assert_eq!(convert_brackets_to_parens("[a, b, c]"), "(a, b, c)");
		// Single element
		assert_eq!(convert_brackets_to_parens("[x]"), "(x)");
		// With whitespace
		assert_eq!(convert_brackets_to_parens("  [a, b]  "), "(a, b)");
		// Empty array
		assert_eq!(convert_brackets_to_parens("[]"), "()");
		// Non-array input unchanged
		assert_eq!(convert_brackets_to_parens("hello"), "hello");
		assert_eq!(convert_brackets_to_parens("(a, b)"), "(a, b)");
		// Nested brackets preserved in content
		assert_eq!(convert_brackets_to_parens("[[1, 2], [3, 4]]"), "([1, 2], [3, 4])");
	}

	#[test]
	fn constant_display_works() {
		let value = Value::u128(250).map_context(|_| 0u32);
		let constant = Constant {
			pallet: "System".to_string(),
			name: "BlockHashCount".to_string(),
			docs: "Maximum number of block number to block hash mappings to keep.".to_string(),
			value,
		};
		assert_eq!(format!("{constant}"), "BlockHashCount");
	}

	#[test]
	fn constant_struct_fields_work() {
		let value = Value::u128(100).map_context(|_| 0u32);
		let constant = Constant {
			pallet: "Balances".to_string(),
			name: "ExistentialDeposit".to_string(),
			docs: "The minimum amount required to keep an account open.".to_string(),
			value: value.clone(),
		};
		assert_eq!(constant.pallet, "Balances");
		assert_eq!(constant.name, "ExistentialDeposit");
		assert_eq!(constant.docs, "The minimum amount required to keep an account open.");
		assert_eq!(constant.value, value);
	}

	#[test]
	fn storage_display_works() {
		let storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "The full account information for a particular account ID.".to_string(),
			type_id: 42,
			key_id: None,
			query_all: false,
		};
		assert_eq!(format!("{storage}"), "Account");
	}

	#[test]
	fn pallet_with_constants_and_storage() {
		// Create a test value using map_context to convert Value<()> to Value<u32>
		let value = Value::u128(250).map_context(|_| 0u32);
		let pallet = Pallet {
			name: "System".to_string(),
			index: 0,
			docs: "System pallet".to_string(),
			functions: vec![],
			constants: vec![Constant {
				pallet: "System".to_string(),
				name: "BlockHashCount".to_string(),
				docs: "Maximum number of block number to block hash mappings to keep.".to_string(),
				value,
			}],
			state: vec![Storage {
				pallet: "System".to_string(),
				name: "Account".to_string(),
				docs: "The full account information for a particular account ID.".to_string(),
				type_id: 42,
				key_id: None,
				query_all: false,
			}],
		};
		assert_eq!(pallet.constants.len(), 1);
		assert_eq!(pallet.state.len(), 1);
		assert_eq!(pallet.constants[0].name, "BlockHashCount");
		assert_eq!(pallet.state[0].name, "Account");
	}

	#[test]
	fn storage_struct_with_key_id_works() {
		// Test storage without key_id (plain storage)
		let plain_storage = Storage {
			pallet: "Timestamp".to_string(),
			name: "Now".to_string(),
			docs: "Current time for the current block.".to_string(),
			type_id: 10,
			key_id: None,
			query_all: false,
		};
		assert_eq!(plain_storage.pallet, "Timestamp");
		assert_eq!(plain_storage.name, "Now");
		assert!(plain_storage.key_id.is_none());

		// Test storage with key_id (storage map)
		let map_storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "The full account information for a particular account ID.".to_string(),
			type_id: 42,
			key_id: Some(100),
			query_all: false,
		};
		assert_eq!(map_storage.pallet, "System");
		assert_eq!(map_storage.name, "Account");
		assert_eq!(map_storage.key_id, Some(100));
	}

	#[test]
	fn raw_value_to_string_works() -> Result<()> {
		// Test simple integer value
		let value = Value::u128(250).map_context(|_| 0u32);
		let result = raw_value_to_string(&value, "")?;
		assert_eq!(result, "250");

		// Test boolean value
		let value = Value::bool(true).map_context(|_| 0u32);
		let result = raw_value_to_string(&value, "")?;
		assert_eq!(result, "true");

		// Test string value
		let value = Value::string("hello").map_context(|_| 0u32);
		let result = raw_value_to_string(&value, "")?;
		assert_eq!(result, "\"hello\"");

		// Test single-element tuple (should unwrap) - demonstrates format_single_tuples
		let inner = Value::u128(42);
		let value = Value::unnamed_composite(vec![inner]).map_context(|_| 0u32);
		let result = raw_value_to_string(&value, "")?;
		assert_eq!(result, "0x2a"); // 42 in hex - unwrapped from tuple

		// Test multi-element composite - hex formatted
		let value =
			Value::unnamed_composite(vec![Value::u128(1), Value::u128(2)]).map_context(|_| 0u32);
		let result = raw_value_to_string(&value, "")?;
		assert_eq!(result, "0x0102"); // Formatted as hex bytes

		Ok(())
	}

	#[test]
	fn call_item_default_works() {
		let item = CallItem::default();
		assert!(matches!(item, CallItem::Function(_)));
		if let CallItem::Function(f) = item {
			assert_eq!(f, Function::default());
		}
	}

	#[test]
	fn call_item_display_works() {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			..Default::default()
		};
		let item = CallItem::Function(function);
		assert_eq!(format!("{item}"), "remark");

		let constant = Constant {
			pallet: "System".to_string(),
			name: "BlockHashCount".to_string(),
			docs: "docs".to_string(),
			value: Value::u128(250).map_context(|_| 0u32),
		};
		let item = CallItem::Constant(constant);
		assert_eq!(format!("{item}"), "BlockHashCount");

		let storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "docs".to_string(),
			type_id: 42,
			key_id: None,
			query_all: false,
		};
		let item = CallItem::Storage(storage);
		assert_eq!(format!("{item}"), "Account");
	}

	#[test]
	fn call_item_as_methods_work() {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			..Default::default()
		};
		let item = CallItem::Function(function.clone());
		assert_eq!(item.as_function(), Some(&function));
		assert_eq!(item.as_constant(), None);
		assert_eq!(item.as_storage(), None);

		let constant = Constant {
			pallet: "System".to_string(),
			name: "BlockHashCount".to_string(),
			docs: "docs".to_string(),
			value: Value::u128(250).map_context(|_| 0u32),
		};
		let item = CallItem::Constant(constant.clone());
		assert_eq!(item.as_function(), None);
		assert_eq!(item.as_constant(), Some(&constant));
		assert_eq!(item.as_storage(), None);

		let storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "docs".to_string(),
			type_id: 42,
			key_id: None,
			query_all: false,
		};
		let item = CallItem::Storage(storage.clone());
		assert_eq!(item.as_function(), None);
		assert_eq!(item.as_constant(), None);
		assert_eq!(item.as_storage(), Some(&storage));
	}

	#[test]
	fn call_item_name_works() {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			..Default::default()
		};
		let item = CallItem::Function(function);
		assert_eq!(item.name(), "remark");

		let constant = Constant {
			pallet: "System".to_string(),
			name: "BlockHashCount".to_string(),
			docs: "docs".to_string(),
			value: Value::u128(250).map_context(|_| 0u32),
		};
		let item = CallItem::Constant(constant);
		assert_eq!(item.name(), "BlockHashCount");

		let storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "docs".to_string(),
			type_id: 42,
			key_id: None,
			query_all: false,
		};
		let item = CallItem::Storage(storage);
		assert_eq!(item.name(), "Account");
	}

	#[test]
	fn call_item_hint_works() {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			..Default::default()
		};
		let item = CallItem::Function(function);
		assert_eq!(item.hint(), "📝 [EXTRINSIC]");

		let constant = Constant {
			pallet: "System".to_string(),
			name: "BlockHashCount".to_string(),
			docs: "docs".to_string(),
			value: Value::u128(250).map_context(|_| 0u32),
		};
		let item = CallItem::Constant(constant);
		assert_eq!(item.hint(), "[CONSTANT]");

		let storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "docs".to_string(),
			type_id: 42,
			key_id: None,
			query_all: false,
		};
		let item = CallItem::Storage(storage);
		assert_eq!(item.hint(), "[STORAGE]");
	}

	#[test]
	fn call_item_docs_works() {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			docs: "Make some on-chain remark.".to_string(),
			..Default::default()
		};
		let item = CallItem::Function(function);
		assert_eq!(item.docs(), "Make some on-chain remark.");

		let constant = Constant {
			pallet: "System".to_string(),
			name: "BlockHashCount".to_string(),
			docs: "Maximum number of block number to block hash mappings to keep.".to_string(),
			value: Value::u128(250).map_context(|_| 0u32),
		};
		let item = CallItem::Constant(constant);
		assert_eq!(item.docs(), "Maximum number of block number to block hash mappings to keep.");

		let storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "The full account information for a particular account ID.".to_string(),
			type_id: 42,
			key_id: None,
			query_all: false,
		};
		let item = CallItem::Storage(storage);
		assert_eq!(item.docs(), "The full account information for a particular account ID.");
	}

	#[test]
	fn call_item_pallet_works() {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			..Default::default()
		};
		let item = CallItem::Function(function);
		assert_eq!(item.pallet(), "System");

		let constant = Constant {
			pallet: "Balances".to_string(),
			name: "ExistentialDeposit".to_string(),
			docs: "docs".to_string(),
			value: Value::u128(100).map_context(|_| 0u32),
		};
		let item = CallItem::Constant(constant);
		assert_eq!(item.pallet(), "Balances");

		let storage = Storage {
			pallet: "Timestamp".to_string(),
			name: "Now".to_string(),
			docs: "docs".to_string(),
			type_id: 10,
			key_id: None,
			query_all: false,
		};
		let item = CallItem::Storage(storage);
		assert_eq!(item.pallet(), "Timestamp");
	}

	#[test]
	fn pallet_get_all_callables_works() {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			..Default::default()
		};
		let constant = Constant {
			pallet: "System".to_string(),
			name: "BlockHashCount".to_string(),
			docs: "docs".to_string(),
			value: Value::u128(250).map_context(|_| 0u32),
		};
		let storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "docs".to_string(),
			type_id: 42,
			key_id: None,
			query_all: false,
		};

		let pallet = Pallet {
			name: "System".to_string(),
			index: 0,
			docs: "System pallet".to_string(),
			functions: vec![function.clone()],
			constants: vec![constant.clone()],
			state: vec![storage.clone()],
		};

		let callables = pallet.get_all_callables();
		assert_eq!(callables.len(), 3);
		assert!(matches!(callables[0], CallItem::Function(_)));
		assert!(matches!(callables[1], CallItem::Constant(_)));
		assert!(matches!(callables[2], CallItem::Storage(_)));

		// Verify the items match
		if let CallItem::Function(f) = &callables[0] {
			assert_eq!(f, &function);
		}
		if let CallItem::Constant(c) = &callables[1] {
			assert_eq!(c, &constant);
		}
		if let CallItem::Storage(s) = &callables[2] {
			assert_eq!(s, &storage);
		}
	}

	#[test]
	fn find_callable_by_name_works() {
		let function = Function {
			pallet: "System".to_string(),
			name: "remark".to_string(),
			..Default::default()
		};
		let constant = Constant {
			pallet: "System".to_string(),
			name: "BlockHashCount".to_string(),
			docs: "docs".to_string(),
			value: Value::u128(250).map_context(|_| 0u32),
		};
		let storage = Storage {
			pallet: "System".to_string(),
			name: "Account".to_string(),
			docs: "docs".to_string(),
			type_id: 42,
			key_id: None,
			query_all: false,
		};

		let pallets = vec![Pallet {
			name: "System".to_string(),
			index: 0,
			docs: "System pallet".to_string(),
			functions: vec![function.clone()],
			constants: vec![constant.clone()],
			state: vec![storage.clone()],
		}];

		// Test finding a function
		let result = find_callable_by_name(&pallets, "System", "remark");
		assert!(result.is_ok());
		if let Ok(CallItem::Function(f)) = result {
			assert_eq!(f.name, "remark");
		}

		// Test finding a constant
		let result = find_callable_by_name(&pallets, "System", "BlockHashCount");
		assert!(result.is_ok());
		if let Ok(CallItem::Constant(c)) = result {
			assert_eq!(c.name, "BlockHashCount");
		}

		// Test finding a storage item
		let result = find_callable_by_name(&pallets, "System", "Account");
		assert!(result.is_ok());
		if let Ok(CallItem::Storage(s)) = result {
			assert_eq!(s.name, "Account");
		}

		// Test not finding a callable
		let result = find_callable_by_name(&pallets, "System", "NonExistent");
		assert!(result.is_err());
		assert!(matches!(result.unwrap_err(), Error::FunctionNotFound(_)));

		// Test pallet not found
		let result = find_callable_by_name(&pallets, "NonExistent", "remark");
		assert!(result.is_err());
		assert!(matches!(result.unwrap_err(), Error::PalletNotFound(_)));
	}

	#[test]
	fn format_single_tuples_single_element_works() -> Result<()> {
		// Create a single-element tuple
		let inner_value = Value::u128(42);
		let single_tuple = Value::unnamed_composite(vec![inner_value]).map_context(|_| 0u32);

		let mut output = String::new();
		let result = format_single_tuples(&single_tuple, &mut output);

		// Should return Some(Ok(())) and unwrap the tuple
		assert!(result.is_some());
		assert!(result.unwrap().is_ok());
		assert_eq!(output, "42");
		Ok(())
	}

	#[test]
	fn format_single_tuples_multi_element_returns_none() -> Result<()> {
		// Create a multi-element tuple
		let tuple =
			Value::unnamed_composite(vec![Value::u128(1), Value::u128(2)]).map_context(|_| 0u32);

		let mut output = String::new();
		let result = format_single_tuples(&tuple, &mut output);

		// Should return None for multi-element tuples
		assert!(result.is_none());
		assert_eq!(output, "");
		Ok(())
	}

	#[test]
	fn format_single_tuples_empty_tuple_returns_none() -> Result<()> {
		// Create an empty tuple
		let empty_tuple = Value::unnamed_composite(vec![]).map_context(|_| 0u32);

		let mut output = String::new();
		let result = format_single_tuples(&empty_tuple, &mut output);

		// Should return None for empty tuples
		assert!(result.is_none());
		assert_eq!(output, "");
		Ok(())
	}

	#[test]
	fn format_single_tuples_non_composite_returns_none() -> Result<()> {
		// Create a non-composite value (not a tuple)
		let simple_value = Value::u128(42).map_context(|_| 0u32);

		let mut output = String::new();
		let result = format_single_tuples(&simple_value, &mut output);

		// Should return None for non-composite values
		assert!(result.is_none());
		assert_eq!(output, "");
		Ok(())
	}

	#[test]
	fn format_single_tuples_named_composite_returns_none() -> Result<()> {
		// Create a named composite (not an unnamed tuple)
		let named_composite =
			Value::named_composite(vec![("field", Value::u128(42))]).map_context(|_| 0u32);

		let mut output = String::new();
		let result = format_single_tuples(&named_composite, &mut output);

		// Should return None for named composites
		assert!(result.is_none());
		assert_eq!(output, "");
		Ok(())
	}

	#[tokio::test]
	async fn query_storage_works() -> Result<()> {
		use crate::{parse_chain_metadata, set_up_client};
		use pop_common::test_env::TestNode;

		// Spawn a test node
		let node = TestNode::spawn().await?;
		let client = set_up_client(node.ws_url()).await?;
		let pallets = parse_chain_metadata(&client)?;

		// Find a storage item (System::Number is a simple storage item that always exists)
		let storage = pallets
			.iter()
			.find(|p| p.name == "System")
			.and_then(|p| p.state.iter().find(|s| s.name == "Number"))
			.expect("System::Number storage should exist");

		// Query the storage (without keys for plain storage)
		let result = storage.query(&client, vec![]).await?;

		// Should return Some value (block number)
		assert!(result.is_some());
		let value = result.unwrap();
		// The value should be decodable as a block number (u32 or u64)
		assert!(matches!(value.value, ValueDef::Primitive(_)));
		Ok(())
	}

	#[tokio::test]
	async fn query_storage_with_key_works() -> Result<()> {
		use crate::{parse_chain_metadata, set_up_client};
		use pop_common::test_env::TestNode;

		// Spawn a test node
		let node = TestNode::spawn().await?;
		let client = set_up_client(node.ws_url()).await?;
		let pallets = parse_chain_metadata(&client)?;

		// Find a map storage item (System::Account requires a key)
		let storage = pallets
			.iter()
			.find(|p| p.name == "System")
			.and_then(|p| p.state.iter().find(|s| s.name == "Account"))
			.expect("System::Account storage should exist");

		// Use Alice's account as the key
		let alice_address = "5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY";
		let account_key = scale_value::stringify::from_str_custom()
			.add_custom_parser(custom_parsers::parse_ss58)
			.parse(alice_address)
			.0
			.expect("Should parse Alice's address");

		// Query the storage with the account key
		let result = storage.query(&client, vec![account_key]).await?;

		// Should return Some value for Alice's account (which should exist in a test chain)
		assert!(result.is_some());
		Ok(())
	}

	#[test]
	fn render_storage_key_values_with_keys_works() -> Result<()> {
		// Create test data with keys
		let key1 = Value::u128(42);
		let key2 = Value::string("test_key");
		let value = Value::bool(true).map_context(|_| 0u32);

		let key_value_pairs = vec![(vec![key1, key2], value)];

		let result = render_storage_key_values(&key_value_pairs)?;

		// Expected format with keys
		let expected = "[\n  42,\n  \"test_key\",\n  true\n]\n";
		assert_eq!(result, expected);
		Ok(())
	}

	#[test]
	fn render_storage_key_values_without_keys_works() -> Result<()> {
		// Create test data without keys (empty key vector)
		let value = Value::u128(100).map_context(|_| 0u32);

		let key_value_pairs = vec![(vec![], value)];

		let result = render_storage_key_values(&key_value_pairs)?;

		// Expected format without keys
		let expected = "[\n  100\n]\n";
		assert_eq!(result, expected);
		Ok(())
	}
}
