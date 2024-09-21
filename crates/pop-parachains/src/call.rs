// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use pop_common::create_signer;
use scale_info::{form::PortableForm, Variant};
use scale_typegen_description::type_description;
use scale_value::{stringify, Value};
use subxt::{metadata::types::StorageEntryType, Metadata, OnlineClient, SubstrateConfig};

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
	args: Vec<String>,
	url: &str,
) -> Result<String, Error> {
	let args_value: Vec<Value> =
		args.into_iter().map(|v| stringify::from_str(&v).0.unwrap()).collect();
	let api = OnlineClient::<SubstrateConfig>::from_url(url).await?;
	let storage_query = subxt::dynamic::storage(pallet_name, entry_name, args_value);
	let result = api.storage().at_latest().await?.fetch(&storage_query).await?;
	if result.is_none() {
		Ok("".to_string())
	} else {
		Ok(result.unwrap().to_value()?.to_string())
	}
}

pub async fn submit_extrinsic(
	pallet_name: &str,
	entry_name: &str,
	args: Vec<String>,
	url: &str,
	suri: &str,
) -> Result<String, Error> {
	let args_value: Vec<Value> = args
		.into_iter()
		.filter_map(|v| match stringify::from_str(&v).0 {
			Ok(value) => Some(value),
			Err(_) => None,
		})
		.collect();
	let api = OnlineClient::<SubstrateConfig>::from_url(url).await?;
	let tx = subxt::dynamic::tx(pallet_name, entry_name, args_value);
	let signer = create_signer(suri)?;
	let result = api
		.tx()
		.sign_and_submit_then_watch_default(&tx, &signer)
		.await?
		.wait_for_finalized_success()
		.await?;
	Ok(result.extrinsic_hash().to_string())
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
	async fn query_works() -> Result<()> {
		let result =
			query("Assets", "Asset", vec!["50".into()], "wss://rpc2.paseo.popnetwork.xyz").await?;
		println!("{:?}", result);
		// query("Nfts", "Collection", &metadata)?;
		// query("Nfts", "NextCollectionId", &metadata)?;

		Ok(())
	}

	#[tokio::test]
	async fn extrinsic_works() -> Result<()> {
		let result = query(
			"Balances",
			"TransferAllowDeath",
			vec!["167Y1SbQrwQVNfkNUXtRkocfzVbaAHYjnZPkZRScWPQ46XDb".into(), "1".into()],
			"wss://rpc2.paseo.popnetwork.xyz",
		)
		.await?;
		println!("{:?}", result);
		// query("Nfts", "Collection", &metadata)?;
		// query("Nfts", "NextCollectionId", &metadata)?;

		Ok(())
	}
}
