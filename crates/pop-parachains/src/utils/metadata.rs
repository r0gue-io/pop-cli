// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::parse_account;
use scale_info::{
	form::PortableForm, Field, PortableRegistry, Type, TypeDef, TypeDefCompact, TypeDefComposite,
	TypeDefPrimitive, TypeDefTuple, TypeDefVariant, Variant,
};
use subxt::{dynamic::Value, Metadata, OnlineClient, SubstrateConfig};

#[derive(Clone, PartialEq, Eq)]
/// Describes a pallet with its extrinsics.
pub struct Pallet {
	/// The name of the pallet.
	pub name: String,
	/// The documentation of the pallet.
	pub docs: String,
	// The extrinsics of the pallet.
	pub extrinsics: Vec<Variant<PortableForm>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arg {
	pub name: String,
	pub type_input: String,
	pub optional: bool,
	pub options: Vec<Arg>,
	pub variant: bool,
}

/// Parses the chain metadata to extract information about pallets and their extrinsics.
pub async fn parse_chain_metadata(
	api: OnlineClient<SubstrateConfig>,
) -> Result<Vec<Pallet>, Error> {
	let metadata: Metadata = api.metadata();
	Ok(metadata
		.pallets()
		.map(|pallet| {
			let extrinsics =
				pallet.call_variants().map(|variants| variants.to_vec()).unwrap_or_default();
			Pallet { name: pallet.name().to_string(), extrinsics, docs: pallet.docs().join(" ") }
		})
		.collect())
}

pub async fn find_pallet_by_name(
	api: &OnlineClient<SubstrateConfig>,
	pallet_name: &str,
) -> Result<Pallet, Error> {
	let metadata: Metadata = api.metadata();
	for pallet in metadata.pallets() {
		if pallet.name() == pallet_name {
			let extrinsics =
				pallet.call_variants().map(|variants| variants.to_vec()).unwrap_or_default();
			return Ok(Pallet {
				name: pallet.name().to_string(),
				extrinsics,
				docs: pallet.docs().join(" "),
			});
		}
	}
	Err(Error::PalletNotFound(pallet_name.to_string()))
}

async fn find_extrinsic_by_name(
	api: &OnlineClient<SubstrateConfig>,
	pallet_name: &str,
	extrinsic_name: &str,
) -> Result<Variant<PortableForm>, Error> {
	let pallet = find_pallet_by_name(api, pallet_name).await?;
	// Check if the specified extrinsic exists within this pallet
	if let Some(extrinsic) = pallet.extrinsics.iter().find(|&e| e.name == extrinsic_name) {
		return Ok(extrinsic.clone());
	} else {
		return Err(Error::ExtrinsicNotSupported(extrinsic_name.to_string()));
	}
}

pub async fn process_extrinsic_args(
	api: &OnlineClient<SubstrateConfig>,
	pallet_name: &str,
	extrinsic_name: &str,
	args: Vec<String>,
) -> Result<Vec<Value>, Error> {
	let metadata: Metadata = api.metadata();
	let registry = metadata.types();
	let extrinsic = find_extrinsic_by_name(&api, pallet_name, extrinsic_name).await?;

	let mut return_args: Vec<Value> = Vec::new();
	for (index, field) in extrinsic.fields.iter().enumerate() {
		let arg_input = args.get(index).ok_or(Error::ParsingArgsError)?;
		let type_info = registry.resolve(field.ty.id).ok_or(Error::ParsingArgsError)?; //Resolve with type_id
		let arg_processed = process_value(arg_input, type_info, registry)?;
		return_args.push(arg_processed);
	}
	Ok(return_args)
}

pub fn process_value(
	arg: &str,
	ty: &Type<PortableForm>,
	registry: &PortableRegistry,
) -> Result<Value, Error> {
	let type_path = ty.path.segments.join("::");
	match type_path.as_str() {
		"Option" => handle_option_type(arg, ty, registry),
		"sp_core::crypto::AccountId32" => Ok(Value::from_bytes(parse_account(arg)?)),
		_ => match &ty.type_def {
			TypeDef::Primitive(primitive) => handle_primitive_type(arg, primitive),
			TypeDef::Composite(composite) => handle_composite_type(arg, composite, registry),
			TypeDef::Variant(variant_def) => handle_variant_type(arg, variant_def, registry),
			TypeDef::Tuple(tuple) => handle_tuple_type(arg, tuple, registry),
			TypeDef::Compact(compact) => handle_compact_type(arg, compact, registry),
			TypeDef::Sequence(_) | TypeDef::Array(_) => Ok(Value::from_bytes(arg)),
			_ => Err(Error::ParsingArgsError),
		},
	}
}
fn handle_option_type(
	arg: &str,
	ty: &Type<PortableForm>,
	registry: &PortableRegistry,
) -> Result<Value, Error> {
	// Handle Option<T>
	if arg.trim() == "None" {
		Ok(Value::unnamed_variant("None", vec![]))
	} else if arg.trim().starts_with("Some(") && arg.trim().ends_with(')') {
		let inner_arg = &arg.trim()[5..arg.trim().len() - 1];
		if let Some(inner_type_id) = ty.type_params.get(0).and_then(|param| param.ty) {
			let inner_ty = registry.resolve(inner_type_id.id()).ok_or(Error::ParsingArgsError)?;
			let inner_value = process_value(inner_arg.trim(), inner_ty, registry)?;
			Ok(Value::unnamed_variant("Some", vec![inner_value]))
		} else {
			Err(Error::ParsingArgsError)
		}
	} else {
		Err(Error::ParsingArgsError)
	}
}

fn handle_primitive_type(arg: &str, primitive: &TypeDefPrimitive) -> Result<Value, Error> {
	match primitive {
		TypeDefPrimitive::Bool => {
			Ok(Value::bool(arg.parse::<bool>().map_err(|_| Error::ParsingArgsError)?))
		},
		TypeDefPrimitive::Char => {
			Ok(Value::char(arg.chars().next().ok_or(Error::ParsingArgsError)?))
		},
		TypeDefPrimitive::Str => Ok(Value::string(arg.to_string())),
		TypeDefPrimitive::U8
		| TypeDefPrimitive::U16
		| TypeDefPrimitive::U32
		| TypeDefPrimitive::U64
		| TypeDefPrimitive::U128
		| TypeDefPrimitive::U256 => {
			Ok(Value::u128(arg.parse::<u128>().map_err(|_| Error::ParsingArgsError)?))
		},
		TypeDefPrimitive::I8
		| TypeDefPrimitive::I16
		| TypeDefPrimitive::I32
		| TypeDefPrimitive::I64
		| TypeDefPrimitive::I128
		| TypeDefPrimitive::I256 => {
			Ok(Value::i128(arg.parse::<i128>().map_err(|_| Error::ParsingArgsError)?))
		},
	}
}
fn handle_composite_type(
	arg: &str,
	composite: &TypeDefComposite<PortableForm>,
	registry: &PortableRegistry,
) -> Result<Value, Error> {
	let arg_trimmed = arg.trim();
	let inner = if arg_trimmed.starts_with('{') && arg_trimmed.ends_with('}') {
		&arg_trimmed[1..arg_trimmed.len() - 1]
	} else {
		arg_trimmed
	};
	let sub_args = split_top_level_commas(inner)?;
	if sub_args.len() != composite.fields.len() {
		return Err(Error::ParsingArgsError);
	}
	let mut values = Vec::new();
	for (field, sub_arg) in composite.fields.iter().zip(sub_args.iter()) {
		let sub_ty = registry.resolve(field.ty.id).ok_or(Error::ParsingArgsError)?;
		let value = process_value(sub_arg.trim(), sub_ty, registry)?;
		let field_name = field.name.clone().unwrap_or_default();
		values.push((field_name, value));
	}
	Ok(Value::named_composite(values))
}
fn handle_variant_type(
	arg: &str,
	variant: &TypeDefVariant<PortableForm>,
	registry: &PortableRegistry,
) -> Result<Value, Error> {
	// Handle variants like Some(value1, value2, ...)
	let input = arg.trim();
	let (variant_name, variant_data) = if let Some(start) = input.find('(') {
		if !input.ends_with(')') {
			return Err(Error::ParsingArgsError);
		}
		let name = input[..start].trim();
		let data_str = &input[start + 1..input.len() - 1];
		(name, Some(data_str))
	} else {
		let name = input.trim();
		(name, None)
	};

	// Find the variant definition
	let variant_def = variant
		.variants
		.iter()
		.find(|v| v.name == variant_name)
		.ok_or(Error::ParsingArgsError)?;

	// Handle variant fields
	let fields_values = if let Some(data_str) = variant_data {
		let inputs = split_top_level_commas(data_str)?;
		if inputs.len() != variant_def.fields.len() {
			return Err(Error::ParsingArgsError);
		}

		let mut values = Vec::new();
		for (field_def, field_input) in variant_def.fields.iter().zip(inputs.iter()) {
			let field_ty = registry.resolve(field_def.ty.id).ok_or(Error::ParsingArgsError)?;
			let field_value = process_value(field_input.trim(), field_ty, registry)?;
			values.push(field_value);
		}
		values
	} else if variant_def.fields.is_empty() {
		vec![]
	} else {
		// Variant has fields but no data provided
		return Err(Error::ParsingArgsError);
	};

	Ok(Value::unnamed_variant(variant_name, fields_values))
}
fn handle_tuple_type(
	arg: &str,
	tuple: &TypeDefTuple<PortableForm>,
	registry: &PortableRegistry,
) -> Result<Value, Error> {
	let arg_trimmed = arg.trim();
	let inner = if arg_trimmed.starts_with('(') && arg_trimmed.ends_with(')') {
		&arg_trimmed[1..arg_trimmed.len() - 1]
	} else {
		arg_trimmed
	};
	let sub_args = split_top_level_commas(inner)?;
	if sub_args.len() != tuple.fields.len() {
		return Err(Error::ParsingArgsError);
	}
	let mut values = Vec::new();
	for (sub_ty_id, sub_arg) in tuple.fields.iter().zip(sub_args.iter()) {
		let sub_ty = registry.resolve(sub_ty_id.id()).ok_or(Error::ParsingArgsError)?;
		let value = process_value(sub_arg.trim(), sub_ty, registry)?;
		values.push(value);
	}
	Ok(Value::unnamed_composite(values))
}
fn handle_compact_type(
	arg: &str,
	compact: &TypeDefCompact<PortableForm>,
	registry: &PortableRegistry,
) -> Result<Value, Error> {
	let inner_ty = registry.resolve(compact.type_param.id()).ok_or(Error::ParsingArgsError)?;
	process_value(arg, inner_ty, registry)
}
fn split_top_level_commas(s: &str) -> Result<Vec<&str>, Error> {
	let mut result = Vec::new();
	let mut brace_depth = 0;
	let mut paren_depth = 0;
	let mut last_index = 0;
	for (i, c) in s.char_indices() {
		match c {
			'{' => brace_depth += 1,
			'}' => brace_depth -= 1,
			'(' => paren_depth += 1,
			')' => paren_depth -= 1,
			',' if brace_depth == 0 && paren_depth == 0 => {
				result.push(&s[last_index..i]);
				last_index = i + 1;
			},
			_ => (),
		}
	}
	if brace_depth != 0 || paren_depth != 0 {
		return Err(Error::ParsingArgsError);
	}
	result.push(&s[last_index..]);
	Ok(result)
}

/// Processes an argument by constructing its `Arg` representation, including type information.
pub fn process_prompt_arguments(
	api: &OnlineClient<SubstrateConfig>,
	field: &Field<PortableForm>,
) -> Result<Arg, Error> {
	let type_id = field.ty().id();
	let metadata = api.metadata();
	let registry = metadata.types();
	let name = format!("{:?}", field.name());
	let type_name = field.type_name();
	parse_type(registry, type_id, name, type_name)
}

fn parse_type(
	registry: &PortableRegistry,
	type_id: u32,
	name: String,
	type_name: Option<&String>,
) -> Result<Arg, Error> {
	let type_info = registry.resolve(type_id).ok_or(Error::ParsingArgsError)?;
	// Check if the type is Option<T> by checking the path segments
	if type_info.path.segments == ["Option"] {
		// The type is Option<T>
		// Get the inner type T from type parameters
		if let Some(inner_type_id) = type_info.type_params.get(0).and_then(|param| param.ty) {
			let inner_arg = parse_type(registry, inner_type_id.id(), name.clone(), type_name)?;
			Ok(Arg {
				name,
				type_input: inner_arg.type_input,
				optional: true,
				options: inner_arg.options,
				variant: false,
			})
		} else {
			// Unable to get inner type
			Err(Error::ParsingArgsError)
		}
	} else {
		let type_input = get_type_name(registry, type_info);
		match &type_info.type_def {
			TypeDef::Primitive(_) => {
				Ok(Arg { name, type_input, optional: false, options: vec![], variant: false })
			},
			TypeDef::Composite(composite) => {
				let mut composite_fields = vec![];

				for composite_field in composite.fields() {
					if let Some(name) = composite_field.name() {
						let field_name = format!("{:?}", composite_field.name());
						let field_type_id = composite_field.ty().id();
						let field_type_name = composite_field.type_name();

						let field_arg =
							parse_type(registry, field_type_id, field_name, field_type_name)?;
						composite_fields.push(field_arg);
					}
				}

				Ok(Arg {
					name,
					type_input,
					optional: false,
					options: composite_fields,
					variant: false,
				})
			},
			TypeDef::Variant(variant) => {
				// Regular enum handling for non-option variants
				let mut variant_fields = vec![];
				for variant in variant.variants() {
					let variant_name = variant.name().to_string();

					let mut fields = vec![];
					for field in variant.fields() {
						let field_name = format!("{:?}", field.name());
						let field_type_id = field.ty().id();
						let field_type_name = field.type_name();

						let field_arg = parse_type(
							registry,
							field_type_id,
							variant_name.clone(),
							field_type_name,
						)?;
						fields.push(field_arg);
					}

					variant_fields.push(Arg {
						name: variant_name,
						type_input: "".to_string(),
						optional: false,
						options: fields,
						variant: false,
					});
				}

				Ok(Arg {
					name,
					type_input,
					optional: false,
					options: variant_fields,
					variant: true,
				})
			},
			TypeDef::Array(_) | TypeDef::Sequence(_) | TypeDef::Tuple(_) | TypeDef::Compact(_) => {
				Ok(Arg { name, type_input, optional: false, options: vec![], variant: false })
			},
			_ => Err(Error::ParsingArgsError),
		}
	}
}

fn get_type_name(registry: &PortableRegistry, type_info: &Type<PortableForm>) -> String {
	if !type_info.path.segments.is_empty() {
		type_info.path.segments.join("::")
	} else {
		match &type_info.type_def {
			TypeDef::Primitive(primitive) => format!("{:?}", primitive),
			TypeDef::Array(array) => {
				// Get the inner type of Compact<T>
				if let Some(inner_type_info) = registry.resolve(array.type_param().id()) {
					get_type_name(registry, inner_type_info)
				} else {
					"Compact<Unknown>".to_string()
				}
			},
			TypeDef::Sequence(sequence) => {
				// Get the inner type of Compact<T>
				if let Some(inner_type_info) = registry.resolve(sequence.type_param().id()) {
					get_type_name(registry, inner_type_info)
				} else {
					"Compact<Unknown>".to_string()
				}
			},
			TypeDef::Tuple(tuple) => {
				let field_types: Vec<String> = tuple
					.fields()
					.iter()
					.map(|field| {
						if let Some(field_type_info) = registry.resolve(field.id()) {
							get_type_name(registry, field_type_info)
						} else {
							"Unknown".to_string()
						}
					})
					.collect();
				format!("({})", field_types.join(", "))
			},
			TypeDef::Compact(compact) => {
				// Get the inner type of Compact<T>
				if let Some(inner_type_info) = registry.resolve(compact.type_param().id()) {
					get_type_name(registry, inner_type_info)
				} else {
					"Compact<Unknown>".to_string()
				}
			},
			_ => "Unknown Type".to_string(),
		}
	}
}

// #[cfg(test)]
// mod tests {
// 	use crate::set_up_api;

// 	use super::*;
// 	use anyhow::Result;

// 	#[tokio::test]
// 	async fn process_prompt_arguments_works() -> Result<()> {
// 		let api = set_up_api("ws://127.0.0.1:9944").await?;
// 		// let ex = find_extrinsic_by_name(&api, "Balances", "transfer_allow_death").await?;
// 		let ex = find_extrinsic_by_name(&api, "Nfts", "mint").await?;
// 		let prompt_args1 = process_prompt_arguments(&api, &ex.fields()[2])?;

// 		Ok(())
// 	}

// 	#[tokio::test]
// 	async fn process_extrinsic_args_works() -> Result<()> {
// 		let api = set_up_api("ws://127.0.0.1:9944").await?;
// 		// let ex = find_extrinsic_by_name(&api, "Balances", "transfer_allow_death").await?;
// 		let ex = find_extrinsic_by_name(&api, "Nfts", "mint").await?;
// 		let args_parsed = process_extrinsic_args(
// 			&api,
// 			"Nfts",
// 			"mint",
// 			vec![
// 				"1".to_string(),
// 				"1".to_string(),
// 				"Id(5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y)".to_string(),
// 				"Some(Some(1), Some(1))".to_string(),
// 			],
// 		)
// 		.await?;
// 		println!(" ARGS PARSER {:?}", args_parsed);

// 		Ok(())
// 	}

// 	#[tokio::test]
// 	async fn process_extrinsic_args2_works() -> Result<()> {
// 		let api = set_up_api("ws://127.0.0.1:9944").await?;
// 		// let ex = find_extrinsic_by_name(&api, "Balances", "transfer_allow_death").await?;
// 		let ex = find_extrinsic_by_name(&api, "Nfts", "mint").await?;
// 		let args_parsed =
// 			process_extrinsic_args(&api, "System", "remark", vec!["0x11".to_string()]).await?;
// 		println!(" ARGS PARSER {:?}", args_parsed);

// 		Ok(())
// 	}
// }
