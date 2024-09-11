// SPDX-License-Identifier: GPL-3.0

// use subxt_codegen::fetch_metadata;
// use subxt_metadata::Metadata;
use crate::errors::Error;
use scale_info::{form::PortableForm, PortableRegistry, Variant};
use subxt::{
	dynamic::Value,
	error::MetadataError,
	metadata::types::{PalletMetadata, StorageEntryMetadata, StorageMetadata},
	Metadata, OnlineClient, SubstrateConfig,
};

#[derive(Clone, PartialEq, Eq)]
pub struct Storage {
	pub name: String,
	pub docs: String,
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
	// /// The constants of the pallet.
	// pub consts: Vec<String>,
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
			.map(|metadata| {
				metadata
					.entries()
					.iter()
					.map(|entry| Storage {
						name: entry.name().to_string(),
						docs: entry.docs().concat(),
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

/// Return details about the given storage entry.
pub fn storage_info<'a>(
	pallet_name: &str,
	entry_name: &str,
	metadata: &'a Metadata,
) -> Result<&'a StorageEntryMetadata, Error> {
	let pallet_metadata = metadata
		.pallet_by_name(pallet_name)
		.ok_or(Error::PalletNotFound(pallet_name.to_string()))?;
	let storage_metadata = pallet_metadata
		.storage()
		.ok_or_else(|| MetadataError::StorageNotFoundInPallet(pallet_name.to_owned()))?;
	let storage_entry = storage_metadata
		.entry_by_name(entry_name)
		.ok_or_else(|| MetadataError::StorageEntryNotFound(entry_name.to_owned()))?;
	Ok(storage_entry)
}
