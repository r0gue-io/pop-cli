// SPDX-License-Identifier: GPL-3.0

use scale_info::{form::PortableForm, PortableRegistry, Type, TypeDef, TypeDefPrimitive};

/// Formats a specified type, using the registry to output its full type representation.
///
/// # Arguments
/// * `ty`: A reference to the `Type<PortableForm>` to be formatted.
/// * `registry`: A reference to the `PortableRegistry` to resolve type dependencies.
pub fn format_type(ty: &Type<PortableForm>, registry: &PortableRegistry) -> String {
	let mut name = ty
		.path
		.segments
		.last()
		.map(|s| s.to_owned())
		.unwrap_or_else(|| ty.path.to_string());

	if !ty.type_params.is_empty() {
		let params: Vec<_> = ty
			.type_params
			.iter()
			.filter_map(|p| {
				if let Some(ty) = p.ty {
					registry.resolve(ty.id)
				} else {
					None // Ignore if p.ty is None
				}
			})
			.map(|t| format_type(t, registry))
			.collect();
		name = format!("{name}<{}>", params.join(","));
	}
	name = format!(
		"{name}{}",
		match &ty.type_def {
			TypeDef::Composite(composite) => {
				if composite.fields.is_empty() {
					return "".to_string();
				}

				let mut named = false;
				let fields: Vec<_> = composite
					.fields
					.iter()
					.filter_map(|f| match f.name.as_ref() {
						None => registry.resolve(f.ty.id).map(|t| format_type(t, registry)),
						Some(field) => {
							named = true;
							f.type_name.as_ref().map(|t| format!("{field}: {t}"))
						},
					})
					.collect();
				match named {
					true => format!(" {{ {} }}", fields.join(", ")),
					false => format!(" ({})", fields.join(", ")),
				}
			},
			TypeDef::Variant(variant) => {
				let variants: Vec<_> = variant
					.variants
					.iter()
					.map(|v| {
						if v.fields.is_empty() {
							return v.name.clone();
						}

						let name = v.name.as_str();
						let mut named = false;
						let fields: Vec<_> = v
							.fields
							.iter()
							.filter_map(|f| match f.name.as_ref() {
								None => registry.resolve(f.ty.id).map(|t| format_type(t, registry)),
								Some(field) => {
									named = true;
									f.type_name.as_ref().map(|t| format!("{field}: {t}"))
								},
							})
							.collect();
						format!(
							"{name}{}",
							match named {
								true => format!("{{ {} }}", fields.join(", ")),
								false => format!("({})", fields.join(", ")),
							}
						)
					})
					.collect();
				format!(": {}", variants.join(", "))
			},
			TypeDef::Sequence(sequence) => {
				format!(
					"[{}]",
					format_type(
						registry.resolve(sequence.type_param.id).expect("sequence type not found"),
						registry
					)
				)
			},
			TypeDef::Array(array) => {
				format!(
					"[{};{}]",
					format_type(
						registry.resolve(array.type_param.id).expect("array type not found"),
						registry
					),
					array.len
				)
			},
			TypeDef::Tuple(tuple) => {
				let fields: Vec<_> = tuple
					.fields
					.iter()
					.filter_map(|p| registry.resolve(p.id))
					.map(|t| format_type(t, registry))
					.collect();
				format!("({})", fields.join(","))
			},
			TypeDef::Primitive(primitive) => {
				use TypeDefPrimitive::*;
				match primitive {
					Bool => "bool",
					Char => "char",
					Str => "str",
					U8 => "u8",
					U16 => "u16",
					U32 => "u32",
					U64 => "u64",
					U128 => "u128",
					U256 => "u256",
					I8 => "i8",
					I16 => "i16",
					I32 => "i32",
					I64 => "i64",
					I128 => "i128",
					I256 => "i256",
				}
				.to_string()
			},
			TypeDef::Compact(compact) => {
				format!(
					"Compact<{}>",
					format_type(
						registry.resolve(compact.type_param.id).expect("compact type not found"),
						registry
					)
				)
			},
			TypeDef::BitSequence(_) => {
				"BitSequence".to_string()
			},
		}
	);

	name
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use subxt::{OnlineClient, SubstrateConfig};

	#[tokio::test]
	async fn format_type_works() -> Result<()> {
		let api =
			OnlineClient::<SubstrateConfig>::from_url("wss://rpc1.paseo.popnetwork.xyz").await?;
		let metadata = api.metadata();
		let registry = metadata.types();

		// Validate `Nfts::mint` extrinsic types cover most of cases.
		let nfts_mint_extrinsic =
			metadata.pallet_by_name("Nfts").unwrap().call_variant_by_name("mint").unwrap();
		let nfts_mint_types: Vec<String> = nfts_mint_extrinsic
			.fields
			.iter()
			.map(|field| {
				let type_info = registry.resolve(field.ty.id).unwrap();
				format_type(&type_info, registry)
			})
			.collect();
		assert_eq!(nfts_mint_types.len(), 4);
		assert_eq!(nfts_mint_types[0], "u32"); // collection
		assert_eq!(nfts_mint_types[1], "u32"); // item
		assert_eq!(nfts_mint_types[2], "MultiAddress<AccountId32 ([u8;32]),()>: Id(AccountId32 ([u8;32])), Index(Compact<()>), Raw([u8]), Address32([u8;32]), Address20([u8;20])"); // mint_to
		assert_eq!(nfts_mint_types[3], "Option<MintWitness<u32,u128> { owned_item: Option<ItemId>, mint_price: Option<Balance> }>: None, Some(MintWitness<u32,u128> { owned_item: Option<ItemId>, mint_price: Option<Balance> })"); // witness_data

		//  Validate `System::remark` to cover Sequences.
		let system_remark_extrinsic = metadata
			.pallet_by_name("System")
			.unwrap()
			.call_variant_by_name("remark")
			.unwrap();
		let system_remark_types: Vec<String> = system_remark_extrinsic
			.fields
			.iter()
			.map(|field| {
				let type_info = registry.resolve(field.ty.id).unwrap();
				format_type(&type_info, registry)
			})
			.collect();
		assert_eq!(system_remark_types.len(), 1);
		assert_eq!(system_remark_types[0], "[u8]"); // remark

		// Extrinsic Scheduler::set_retry, cover tuples.
		let scheduler_set_retry_extrinsic = metadata
			.pallet_by_name("Scheduler")
			.unwrap()
			.call_variant_by_name("set_retry")
			.unwrap();
		let scheduler_set_retry_types: Vec<String> = scheduler_set_retry_extrinsic
			.fields
			.iter()
			.map(|field| {
				let type_info = registry.resolve(field.ty.id).unwrap();
				format_type(&type_info, registry)
			})
			.collect();
		assert_eq!(scheduler_set_retry_types.len(), 3);
		assert_eq!(scheduler_set_retry_types[0], "(u32,u32)"); // task
		assert_eq!(scheduler_set_retry_types[1], "u8"); // retries
		assert_eq!(scheduler_set_retry_types[2], "u32"); // period

		Ok(())
	}
}
