// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::parse_account;
use scale_info::{
	form::PortableForm, PortableRegistry, Type, TypeDef, TypeDefArray, TypeDefCompact,
	TypeDefComposite, TypeDefPrimitive, TypeDefSequence, TypeDefTuple, TypeDefVariant,
};
use subxt::dynamic::Value;

/// Parses an argument string into a `Value` based on its type definition.
///
/// # Arguments
/// * `arg`: The string representation of the argument to parse.
/// * `ty`: A reference to the `Type<PortableForm>` to be formatted.
/// * `registry`: A reference to the `PortableRegistry` to resolve type dependencies.
pub fn process_argument(
	arg: &str,
	ty: &Type<PortableForm>,
	registry: &PortableRegistry,
) -> Result<Value, Error> {
	let type_path = ty.path.segments.join("::");
	match type_path.as_str() {
		"Option" => handle_option_type(arg, ty, registry),
		"sp_core::crypto::AccountId32" => Ok(Value::from_bytes(parse_account(arg)?)), /* Specifically parse AccountId */
		_ => match &ty.type_def {
			TypeDef::Primitive(primitive) => primitive.parse(arg, registry),
			TypeDef::Composite(composite) => composite.parse(arg, registry),
			TypeDef::Variant(variant) => variant.parse(arg, registry),
			TypeDef::Tuple(tuple) => tuple.parse(arg, registry),
			TypeDef::Sequence(sequence) => sequence.parse(arg, registry),
			TypeDef::Array(array) => array.parse(arg, registry),
			TypeDef::Compact(compact) => compact.parse(arg, registry),
			_ => Err(Error::ParamProcessingError),
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
		let sub_arg = &arg.trim()[5..arg.trim().len() - 1];
		if let Some(sub_arg_type_id) = ty.type_params.get(0).and_then(|param| param.ty) {
			let sub_arg_ty =
				registry.resolve(sub_arg_type_id.id).ok_or(Error::ParamProcessingError)?;
			Ok(Value::unnamed_variant(
				"Some",
				vec![process_argument(sub_arg.trim(), sub_arg_ty, registry)?],
			))
		} else {
			Err(Error::ParamProcessingError)
		}
	} else {
		Err(Error::ParamProcessingError)
	}
}

/// Trait to define how different type definitions parse a string argument into a `Value`.
pub trait TypeParser {
	fn parse(&self, arg: &str, registry: &PortableRegistry) -> Result<Value, Error>;
}

impl TypeParser for TypeDefPrimitive {
	fn parse(&self, arg: &str, _registry: &PortableRegistry) -> Result<Value, Error> {
		match self {
			TypeDefPrimitive::Bool =>
				Ok(Value::bool(arg.parse::<bool>().map_err(|_| Error::ParamProcessingError)?)),
			TypeDefPrimitive::Char =>
				Ok(Value::char(arg.chars().next().ok_or(Error::ParamProcessingError)?)),
			TypeDefPrimitive::Str => Ok(Value::string(arg.to_string())),
			TypeDefPrimitive::U8 |
			TypeDefPrimitive::U16 |
			TypeDefPrimitive::U32 |
			TypeDefPrimitive::U64 |
			TypeDefPrimitive::U128 |
			TypeDefPrimitive::U256 =>
				Ok(Value::u128(arg.parse::<u128>().map_err(|_| Error::ParamProcessingError)?)),
			TypeDefPrimitive::I8 |
			TypeDefPrimitive::I16 |
			TypeDefPrimitive::I32 |
			TypeDefPrimitive::I64 |
			TypeDefPrimitive::I128 |
			TypeDefPrimitive::I256 =>
				Ok(Value::i128(arg.parse::<i128>().map_err(|_| Error::ParamProcessingError)?)),
		}
	}
}

impl TypeParser for TypeDefVariant<PortableForm> {
	fn parse(&self, arg: &str, registry: &PortableRegistry) -> Result<Value, Error> {
		let input = arg.trim();
		// Parse variant with data (e.g., `Some(value1, value2, ...)`).
		if let Some(start) = input.find('(') {
			if !input.ends_with(')') {
				return Err(Error::ParamProcessingError);
			}
			let name = input[..start].trim();
			let data_str = &input[start + 1..input.len() - 1];
			let variant_def = self
				.variants
				.iter()
				.find(|v| v.name == name)
				.ok_or_else(|| Error::ParamProcessingError)?;

			let mut values = Vec::new();
			for field in variant_def.fields.iter() {
				let field_type_id = field.ty.id;
				let field_ty =
					registry.resolve(field_type_id).ok_or_else(|| Error::ParamProcessingError)?;
				// Recursive for the sub parameters of variant type.
				let field_value = process_argument(data_str, field_ty, registry)?;
				values.push(field_value);
			}

			Ok(Value::unnamed_variant(name.to_string(), values))
		} else {
			// Parse variant without data (e.g., `None`).
			let name = input.to_string();
			let variant_def = self
				.variants
				.iter()
				.find(|v| v.name == name)
				.ok_or_else(|| Error::ParamProcessingError)?;
			if !variant_def.fields.is_empty() {
				return Err(Error::ParamProcessingError);
			}
			Ok(Value::unnamed_variant(name, vec![]))
		}
	}
}

impl TypeParser for TypeDefComposite<PortableForm> {
	// Example: {"a": true, "b": "hello", "c": { "d": 42, e: "world" }}
	fn parse(&self, arg: &str, registry: &PortableRegistry) -> Result<Value, Error> {
		let mut values: Vec<&str> = arg.split(',').map(str::trim).collect();

		let mut field_values = Vec::new();
		for (index, field) in self.fields.iter().enumerate() {
			let field_name = field
				.name
				.clone()
				.or_else(|| field.type_name.clone())
				.unwrap_or_else(|| format!("unnamed_field_{}", index));

			let field_type = registry.resolve(field.ty.id).ok_or(Error::ParamProcessingError)?;
			if values.is_empty() {
				return Err(Error::ParamProcessingError);
			}
			let value = match &field_type.type_def {
				TypeDef::Composite(nested_composite) => {
					if nested_composite.fields.is_empty() {
						// Unnamed composite resolving to a primitive type
						let raw_value = values.remove(0);
						process_argument(raw_value, field_type, registry)?
					} else {
						// Named or unnamed nested composite
						let nested_args_count = nested_composite.fields.len();
						if values.len() < nested_args_count {
							return Err(Error::ParamProcessingError);
						}

						let nested_args: Vec<String> =
							values.drain(..nested_args_count).map(String::from).collect();
						let nested_arg_str = nested_args.join(",");
						nested_composite.parse(&nested_arg_str, registry)?
					}
				},
				_ => {
					// Parse a single argument for non-composite fields
					let raw_value = values.remove(0);
					process_argument(raw_value, field_type, registry)?
				},
			};
			field_values.push((field_name, value));
		}
		Ok(Value::named_composite(field_values))
	}
}

impl TypeParser for TypeDefSequence<PortableForm> {
	// Example: [val1, val2, ...]
	fn parse(&self, arg: &str, _registry: &PortableRegistry) -> Result<Value, Error> {
		Ok(Value::from_bytes(arg))
	}
}

impl TypeParser for TypeDefArray<PortableForm> {
	// Example: [val1, val2, ...]
	fn parse(&self, arg: &str, _registry: &PortableRegistry) -> Result<Value, Error> {
		Ok(Value::from_bytes(arg))
	}
}

impl TypeParser for TypeDefTuple<PortableForm> {
	fn parse(&self, arg: &str, registry: &PortableRegistry) -> Result<Value, Error> {
		let input = arg.trim();
		// Extract tuple contents from parentheses (e.g., `(value1, value2, ...)`).
		let tuple_content = if input.starts_with('(') && input.ends_with(')') {
			&input[1..input.len() - 1]
		} else {
			input
		};
		let tuple_values: Vec<&str> = tuple_content.split(',').map(|s| s.trim()).collect();
		if tuple_values.len() != self.fields.len() {
			return Err(Error::ParamProcessingError);
		}
		let mut values = Vec::new();
		for (sub_ty_id, sub_arg) in self.fields.iter().zip(tuple_values.iter()) {
			let sub_ty =
				registry.resolve(sub_ty_id.id).ok_or_else(|| Error::ParamProcessingError)?;
			// Recursive for each value of the tuple.
			let value = process_argument(sub_arg.trim(), sub_ty, registry)?;
			values.push(value);
		}
		Ok(Value::unnamed_composite(values))
	}
}

impl TypeParser for TypeDefCompact<PortableForm> {
	fn parse(&self, input: &str, registry: &PortableRegistry) -> Result<Value, Error> {
		// Parse compact types as their sub type (e.g., `Compact<T>`).
		let sub_ty = registry
			.resolve(self.type_param.id)
			.ok_or_else(|| Error::ParamProcessingError)?;
		// Recursive for the inner value.
		process_argument(input, sub_ty, registry)
	}
}
