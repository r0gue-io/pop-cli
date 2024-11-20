// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
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
	match &ty.type_def {
		TypeDef::Primitive(primitive) => primitive.parse(arg, registry),
		TypeDef::Composite(composite) => composite.parse(arg, registry),
		TypeDef::Variant(variant) => variant.parse(arg, registry),
		TypeDef::Tuple(tuple) => tuple.parse(arg, registry),
		TypeDef::Sequence(sequence) => sequence.parse(arg, registry),
		TypeDef::Array(array) => array.parse(arg, registry),
		TypeDef::Compact(compact) => compact.parse(arg, registry),
		_ => Err(Error::ParsingArgsError),
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
				Ok(Value::bool(arg.parse::<bool>().map_err(|_| Error::ParsingArgsError)?)),
			TypeDefPrimitive::Char =>
				Ok(Value::char(arg.chars().next().ok_or(Error::ParsingArgsError)?)),
			TypeDefPrimitive::Str => Ok(Value::string(arg.to_string())),
			TypeDefPrimitive::U8 |
			TypeDefPrimitive::U16 |
			TypeDefPrimitive::U32 |
			TypeDefPrimitive::U64 |
			TypeDefPrimitive::U128 |
			TypeDefPrimitive::U256 =>
				Ok(Value::u128(arg.parse::<u128>().map_err(|_| Error::ParsingArgsError)?)),
			TypeDefPrimitive::I8 |
			TypeDefPrimitive::I16 |
			TypeDefPrimitive::I32 |
			TypeDefPrimitive::I64 |
			TypeDefPrimitive::I128 |
			TypeDefPrimitive::I256 =>
				Ok(Value::i128(arg.parse::<i128>().map_err(|_| Error::ParsingArgsError)?)),
		}
	}
}

impl TypeParser for TypeDefVariant<PortableForm> {
	fn parse(&self, arg: &str, registry: &PortableRegistry) -> Result<Value, Error> {
		let input = arg.trim();
		// Parse variant with data (e.g., `Some(value1, value2, ...)`).
		if let Some(start) = input.find('(') {
			if !input.ends_with(')') {
				return Err(Error::ParsingArgsError);
			}
			let name = input[..start].trim();
			let data_str = &input[start + 1..input.len() - 1];
			let variant_def = self
				.variants
				.iter()
				.find(|v| v.name == name)
				.ok_or_else(|| Error::ParsingArgsError)?;

			let mut values = Vec::new();
			for field in variant_def.fields.iter() {
				let field_type_id = field.ty.id;
				let field_ty =
					registry.resolve(field_type_id).ok_or_else(|| Error::ParsingArgsError)?;
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
				.ok_or_else(|| Error::ParsingArgsError)?;
			if !variant_def.fields.is_empty() {
				return Err(Error::ParsingArgsError);
			}
			Ok(Value::unnamed_variant(name, vec![]))
		}
	}
}

impl TypeParser for TypeDefComposite<PortableForm> {
	// Example: {"a": true, "b": "hello"}
	fn parse(&self, arg: &str, _registry: &PortableRegistry) -> Result<Value, Error> {
		let parsed: serde_json::Value =
			serde_json::from_str(arg).map_err(|_| Error::ParsingArgsError)?;
		let scale_val =
			serde_json::from_value::<Value<()>>(parsed).map_err(|_| Error::ParsingArgsError)?;
		Ok(scale_val)
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
			return Err(Error::ParsingArgsError);
		}
		let mut values = Vec::new();
		for (sub_ty_id, sub_arg) in self.fields.iter().zip(tuple_values.iter()) {
			let sub_ty = registry.resolve(sub_ty_id.id).ok_or_else(|| Error::ParsingArgsError)?;
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
		let sub_ty = registry.resolve(self.type_param.id).ok_or_else(|| Error::ParsingArgsError)?;
		// Recursive for the inner value.
		process_argument(input, sub_ty, registry)
	}
}
