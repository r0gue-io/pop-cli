// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::format_type;
use scale_info::{form::PortableForm, Field, PortableRegistry, TypeDef};
use subxt::Metadata;

/// Describes a parameter of a dispatchable function.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Param {
	/// The name of the parameter.
	pub name: String,
	/// The type of the parameter.
	pub type_name: String,
	/// Nested parameters for composite, variants, types or tuples.
	pub sub_params: Vec<Param>,
	/// Indicates if the parameter is optional (`Option<T>`).
	pub is_optional: bool,
	/// Indicates if the parameter is a Tuple.
	pub is_tuple: bool,
	/// Indicates if the parameter is a Variant.
	pub is_variant: bool,
	/// Indicates if the parameter is a Sequence.
	pub is_sequence: bool,
}

/// Transforms a metadata field into its `Param` representation.
///
/// # Arguments
/// * `metadata`: The chain metadata.
/// * `field`: A parameter of a dispatchable function (as [Field]).
pub fn field_to_param(metadata: &Metadata, field: &Field<PortableForm>) -> Result<Param, Error> {
	let registry = metadata.types();
	if let Some(name) = field.type_name.as_deref() {
		if name.contains("RuntimeCall") {
			return Err(Error::FunctionNotSupported);
		}
	}
	let name = field.name.as_deref().unwrap_or("Unnamed"); //It can be unnamed field
	type_to_param(name, registry, field.ty.id)
}

/// Converts a type's metadata into a `Param` representation.
///
/// # Arguments
/// * `name`: The name of the parameter.
/// * `registry`: Type registry containing all types used in the metadata.
/// * `type_id`: The ID of the type to be converted.
fn type_to_param(name: &str, registry: &PortableRegistry, type_id: u32) -> Result<Param, Error> {
	let type_info = registry
		.resolve(type_id)
		.ok_or_else(|| Error::MetadataParsingError(name.to_string()))?;
	// Check for unsupported `RuntimeCall` type
	if type_info.path.segments.contains(&"RuntimeCall".to_string()) {
		return Err(Error::FunctionNotSupported);
	}
	for param in &type_info.type_params {
		if param.name.contains("RuntimeCall") {
			return Err(Error::FunctionNotSupported);
		}
	}
	if type_info.path.segments == ["Option"] {
		if let Some(sub_type_id) = type_info.type_params.first().and_then(|param| param.ty) {
			// Recursive for the sub parameters
			let sub_param = type_to_param(name, registry, sub_type_id.id)?;
			Ok(Param {
				name: name.to_string(),
				type_name: sub_param.type_name,
				sub_params: sub_param.sub_params,
				is_optional: true,
				..Default::default()
			})
		} else {
			Err(Error::MetadataParsingError(name.to_string()))
		}
	} else {
		// Determine the formatted type name.
		let type_name = format_type(type_info, registry);
		match &type_info.type_def {
			TypeDef::Primitive(_) | TypeDef::Array(_) | TypeDef::Compact(_) =>
				Ok(Param { name: name.to_string(), type_name, ..Default::default() }),
			TypeDef::Composite(composite) => {
				let sub_params = composite
					.fields
					.iter()
					.map(|field| {
						// Recursive for the sub parameters of composite type.
						type_to_param(field.name.as_deref().unwrap_or(name), registry, field.ty.id)
					})
					.collect::<Result<Vec<Param>, Error>>()?;

				Ok(Param { name: name.to_string(), type_name, sub_params, ..Default::default() })
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
									field.name.as_deref().unwrap_or(&variant_param.name),
									registry,
									field.ty.id,
								)
							})
							.collect::<Result<Vec<Param>, Error>>()?;
						Ok(Param {
							name: variant_param.name.clone(),
							type_name: "".to_string(),
							sub_params: variant_sub_params,
							is_variant: true,
							..Default::default()
						})
					})
					.collect::<Result<Vec<Param>, Error>>()?;

				Ok(Param {
					name: name.to_string(),
					type_name,
					sub_params: variant_params,
					is_variant: true,
					..Default::default()
				})
			},
			TypeDef::Sequence(_) => Ok(Param {
				name: name.to_string(),
				type_name,
				is_sequence: true,
				..Default::default()
			}),
			TypeDef::Tuple(tuple) => {
				let sub_params = tuple
					.fields
					.iter()
					.enumerate()
					.map(|(index, field_id)| {
						type_to_param(
							&format!("Index {index} of the tuple {name}"),
							registry,
							field_id.id,
						)
					})
					.collect::<Result<Vec<Param>, Error>>()?;

				Ok(Param {
					name: name.to_string(),
					type_name,
					sub_params,
					is_tuple: true,
					..Default::default()
				})
			},
			_ => Err(Error::MetadataParsingError(name.to_string())),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{call::tests::POP_NETWORK_TESTNET_URL, set_up_client};
	use anyhow::Result;

	#[tokio::test]
	async fn field_to_param_works() -> Result<()> {
		let client = set_up_client(POP_NETWORK_TESTNET_URL).await?;
		let metadata = client.metadata();
		// Test a supported dispatchable function.
		let function = metadata
			.pallet_by_name("Balances")
			.unwrap()
			.call_variant_by_name("force_transfer")
			.unwrap();
		let mut params = Vec::new();
		for field in &function.fields {
			params.push(field_to_param(&metadata, field)?)
		}
		assert_eq!(params.len(), 3);
		assert_eq!(params.first().unwrap().name, "source");
		assert_eq!(params.first().unwrap().type_name, "MultiAddress<AccountId32 ([u8;32]),()>: Id(AccountId32 ([u8;32])), Index(Compact<()>), Raw([u8]), Address32([u8;32]), Address20([u8;20])");
		assert_eq!(params.first().unwrap().sub_params.len(), 5);
		assert_eq!(params.first().unwrap().sub_params.first().unwrap().name, "Id");
		assert_eq!(params.first().unwrap().sub_params.first().unwrap().type_name, "");
		assert_eq!(
			params
				.first()
				.unwrap()
				.sub_params
				.first()
				.unwrap()
				.sub_params
				.first()
				.unwrap()
				.name,
			"Id"
		);
		assert_eq!(
			params
				.first()
				.unwrap()
				.sub_params
				.first()
				.unwrap()
				.sub_params
				.first()
				.unwrap()
				.type_name,
			"AccountId32 ([u8;32])"
		);
		// Test some dispatchable functions that are not supported.
		let function =
			metadata.pallet_by_name("Sudo").unwrap().call_variant_by_name("sudo").unwrap();
		assert!(matches!(
			field_to_param(&metadata, &function.fields.first().unwrap()),
			Err(Error::FunctionNotSupported)
		));
		let function = metadata
			.pallet_by_name("Utility")
			.unwrap()
			.call_variant_by_name("batch")
			.unwrap();
		assert!(matches!(
			field_to_param(&metadata, &function.fields.first().unwrap()),
			Err(Error::FunctionNotSupported)
		));
		let function = metadata
			.pallet_by_name("PolkadotXcm")
			.unwrap()
			.call_variant_by_name("execute")
			.unwrap();
		assert!(matches!(
			field_to_param(&metadata, &function.fields.first().unwrap()),
			Err(Error::FunctionNotSupported)
		));
		// TODO: Use the Pop Network endpoint once the mainnet (or an equivalent testnet with the
		// same runtime) is available.
		let client = set_up_client("wss://mythos.ibp.network").await?;
		let metadata = client.metadata();
		let function = metadata
			.pallet_by_name("Council")
			.unwrap()
			.call_variant_by_name("execute")
			.unwrap();
		assert!(matches!(
			field_to_param(&metadata, &function.fields.first().unwrap()),
			Err(Error::FunctionNotSupported)
		));

		Ok(())
	}
}
