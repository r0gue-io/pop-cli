// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::format_type;
use scale_info::{form::PortableForm, Field, PortableRegistry, TypeDef};
use subxt::{Metadata, OnlineClient, SubstrateConfig};

/// Describes a parameter of an extrinsic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
	/// The name of the parameter.
	pub name: String,
	/// The type of the parameter.
	pub type_name: String,
	/// Nested parameters for composite, variants types or tuples.
	pub sub_params: Vec<Param>,
	/// Indicates if the parameter is optional (`Option<T>`).
	pub is_optional: bool,
	/// Indicates if the parameter is a Tuple.
	pub is_tuple: bool,
	/// Indicates if the parameter is a Variant.
	pub is_variant: bool,
}

/// Transforms a metadata field into its `Param` representation.
///
/// # Arguments
/// * `api`: Reference to an `OnlineClient` connected to the blockchain.
/// * `field`: A reference to a metadata field of the extrinsic.
pub fn field_to_param(
	api: &OnlineClient<SubstrateConfig>,
	extrinsic_name: &str,
	field: &Field<PortableForm>,
) -> Result<Param, Error> {
	let metadata: Metadata = api.metadata();
	let registry = metadata.types();
	let name = field.name.clone().unwrap_or("Unnamed".to_string()); //It can be unnamed field
	type_to_param(extrinsic_name, name, registry, field.ty.id, &field.type_name)
}

/// Converts a type's metadata into a `Param` representation.
///
/// # Arguments
/// * `name`: The name of the parameter.
/// * `registry`: A reference to the `PortableRegistry` to resolve type dependencies.
/// * `type_id`: The ID of the type to be converted.
/// * `type_name`: An optional descriptive name for the type.
fn type_to_param(
	extrinsic_name: &str,
	name: String,
	registry: &PortableRegistry,
	type_id: u32,
	type_name: &Option<String>,
) -> Result<Param, Error> {
	let type_info = registry.resolve(type_id).ok_or(Error::MetadataParsingError(name.clone()))?;
	if let Some(last_segment) = type_info.path.segments.last() {
		if last_segment == "RuntimeCall" {
			return Err(Error::ExtrinsicNotSupported(extrinsic_name.to_string()));
		}
	}
	for param in &type_info.type_params {
		if param.name == "RuntimeCall" ||
			param.name == "Vec<RuntimeCall>" ||
			param.name == "Vec<<T as Config>::RuntimeCall>"
		{
			return Err(Error::ExtrinsicNotSupported(extrinsic_name.to_string()));
		}
	}
	if type_info.path.segments == ["Option"] {
		if let Some(sub_type_id) = type_info.type_params.get(0).and_then(|param| param.ty) {
			// Recursive for the sub parameters
			let sub_param =
				type_to_param(extrinsic_name, name.clone(), registry, sub_type_id.id, type_name)?;
			return Ok(Param {
				name,
				type_name: sub_param.type_name,
				sub_params: sub_param.sub_params,
				is_optional: true,
				is_tuple: false,
				is_variant: false,
			});
		} else {
			Err(Error::MetadataParsingError(name))
		}
	} else {
		// Determine the formatted type name.
		let type_name = format_type(type_info, registry);
		match &type_info.type_def {
			TypeDef::Primitive(_) |
			TypeDef::Array(_) |
			TypeDef::Sequence(_) |
			TypeDef::Compact(_) => Ok(Param {
				name,
				type_name,
				sub_params: Vec::new(),
				is_optional: false,
				is_tuple: false,
				is_variant: false,
			}),
			TypeDef::Composite(composite) => {
				let sub_params = composite
					.fields
					.iter()
					.map(|field| {
						// Recursive for the sub parameters of composite type.
						type_to_param(
							extrinsic_name,
							field.name.clone().unwrap_or(name.clone()),
							registry,
							field.ty.id,
							&field.type_name,
						)
					})
					.collect::<Result<Vec<Param>, Error>>()?;

				Ok(Param {
					name,
					type_name,
					sub_params,
					is_optional: false,
					is_tuple: false,
					is_variant: false,
				})
			},
			TypeDef::Variant(variant) => {
				let variant_params = variant
					.variants
					.iter()
					.map(|variant_param| {
						let variant_sub_params = variant_param
							.fields
							.iter()
							.map(|field| {
								// Recursive for the sub parameters of variant type.
								type_to_param(
									extrinsic_name,
									field.name.clone().unwrap_or(variant_param.name.clone()),
									registry,
									field.ty.id,
									&field.type_name,
								)
							})
							.collect::<Result<Vec<Param>, Error>>()?;
						Ok(Param {
							name: variant_param.name.clone(),
							type_name: "".to_string(),
							sub_params: variant_sub_params,
							is_optional: false,
							is_tuple: false,
							is_variant: true,
						})
					})
					.collect::<Result<Vec<Param>, Error>>()?;

				Ok(Param {
					name,
					type_name,
					sub_params: variant_params,
					is_optional: false,
					is_tuple: false,
					is_variant: true,
				})
			},
			TypeDef::Tuple(tuple) => {
				let sub_params = tuple
					.fields
					.iter()
					.enumerate()
					.map(|(index, field_id)| {
						type_to_param(
							extrinsic_name,
							format!("Index {} of the tuple {}", index.to_string(), name),
							registry,
							field_id.id,
							&None,
						)
					})
					.collect::<Result<Vec<Param>, Error>>()?;

				Ok(Param {
					name,
					type_name,
					sub_params,
					is_optional: false,
					is_tuple: true,
					is_variant: false,
				})
			},
			_ => Err(Error::MetadataParsingError(name)),
		}
	}
}
