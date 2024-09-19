// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use scale_info::{form::PortableForm, Variant};
use scale_typegen_description::type_description;
use subxt::{
	dynamic::Value, metadata::types::StorageEntryType, Metadata, OnlineClient, SubstrateConfig,
};

#[derive(Clone, PartialEq, Eq)]
pub struct Storage {
	pub name: String,
	pub docs: String,
	pub ty: (u32, Option<u32>),
}

#[derive(Clone, PartialEq, Eq)]
/// Describes a contract message.
pub struct Pallet {
	/// The label of the message.
	pub label: String,
	/// The message documentation.
	pub docs: String,
	// The extrinsics of the pallet.
	pub extrinsics: Vec<Variant<PortableForm>>,
	// The storage of the pallet.
	pub storage: Vec<Storage>,
}

pub async fn fetch_metadata(url: &str) -> Result<Metadata, Error> {
	let api = OnlineClient::<SubstrateConfig>::from_url(url).await?;
	Ok(api.metadata())
}

pub async fn query(
	pallet_name: &str,
	entry_name: &str,
	args: Vec<Value>,
	url: &str,
) -> Result<(), Error> {
	let api = OnlineClient::<SubstrateConfig>::from_url(url).await?;
	println!("here");
	let storage_query = subxt::dynamic::storage(pallet_name, entry_name, args);
	println!("here");
	let mut results = api.storage().at_latest().await?.iter(storage_query).await?;
	println!("{:?}", results);
	while let Some(Ok(kv)) = results.next().await {
		println!("Keys decoded: {:?}", kv.keys);
		println!("Value: {:?}", kv.value.to_value().map_err(|_| Error::ParsingResponseError)?);
	}
	Ok(())
}

pub async fn parse_chain_metadata(metadata: Metadata) -> Result<Vec<Pallet>, Error> {
	let mut pallets: Vec<Pallet> = Vec::new();
	for pallet in metadata.pallets() {
		let extrinsics =
			pallet.call_variants().map(|variants| variants.to_vec()).unwrap_or_default(); // Return an empty Vec if Option is None
		let storage: Vec<Storage> = pallet
			.storage()
			.map(|m| {
				m.entries()
					.iter()
					.map(|entry| Storage {
						name: entry.name().to_string(),
						docs: entry.docs().concat(),
						ty: match entry.entry_type() {
							StorageEntryType::Plain(value) => (*value, None),
							StorageEntryType::Map { value_ty, key_ty, .. } =>
								(*value_ty, Some(*key_ty)),
						},
					})
					.collect()
			})
			.unwrap_or_default(); // Return an empty Vec if Option is None

		pallets.push(Pallet {
			label: pallet.name().to_string(),
			extrinsics,
			docs: pallet.docs().join(" "),
			storage,
		});
	}
	Ok(pallets)
}

pub fn get_type_description(
	key_ty_id: Option<u32>,
	metadata: &Metadata,
) -> Result<Vec<String>, Error> {
	if let Some(key_ty_id) = key_ty_id {
		let key_ty_description = type_description(key_ty_id, metadata.types(), false)?;
		let result = key_ty_description.trim().trim_matches(|c| c == '(' || c == ')');

		let parsed_result: Vec<String> = if result == "\"\"" {
			vec![]
		} else if !result.contains(',') {
			vec![result.to_string()]
		} else {
			result.split(',').map(|s| s.trim().to_string()).collect()
		};

		Ok(parsed_result)
	} else {
		Ok(vec![])
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[tokio::test]
	async fn storage_info_works() -> Result<()> {
		let metadata = fetch_metadata("wss://rpc2.paseo.popnetwork.xyz").await?;
		explore("Nfts", "Account", &metadata)?;
		explore("Nfts", "Collection", &metadata)?;
		explore("Nfts", "NextCollectionId", &metadata)?;

		Ok(())
	}
}
