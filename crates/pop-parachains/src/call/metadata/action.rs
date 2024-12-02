// SPDX-License-Identifier: GPL-3.0

use super::{find_extrinsic_by_name, Pallet};
use strum::{EnumMessage as _, EnumProperty as _, VariantArray as _};
use strum_macros::{AsRefStr, Display, EnumMessage, EnumProperty, EnumString, VariantArray};

/// Enum representing various predefined actions supported.
#[derive(
	AsRefStr,
	Clone,
	Debug,
	Display,
	EnumMessage,
	EnumString,
	EnumProperty,
	Eq,
	Hash,
	PartialEq,
	VariantArray,
)]
pub enum Action {
	/// Transfer balance.
	#[strum(
		serialize = "transfer",
		message = "transfer_allow_death",
		detailed_message = "Transfer balance",
		props(Pallet = "Balances")
	)]
	Transfer,
	/// Create an asset.
	#[strum(
		serialize = "create",
		message = "create",
		detailed_message = "Create an asset",
		props(Pallet = "Assets")
	)]
	CreateAsset,
	/// Mint an asset.
	#[strum(
		serialize = "mint",
		message = "mint",
		detailed_message = "Mint an asset",
		props(Pallet = "Assets")
	)]
	MintAsset,
	/// Create an NFT collection.
	#[strum(
		serialize = "create_nft",
		message = "create",
		detailed_message = "Create an NFT collection",
		props(Pallet = "Nfts")
	)]
	CreateCollection,
	/// Mint an NFT.
	#[strum(
		serialize = "mint_nft",
		message = "mint",
		detailed_message = "Mint an NFT",
		props(Pallet = "Nfts")
	)]
	MintNFT,
	/// Purchase on-demand coretime.
	#[strum(
		serialize = "place_order_allow_death",
		message = "place_order_allow_death",
		detailed_message = "Purchase on-demand coretime",
		props(Pallet = "OnDemand")
	)]
	PurchaseOnDemandCoretime,
	/// Reserve a parachain ID.
	#[strum(
		serialize = "reserve",
		message = "reserve",
		detailed_message = "Reserve a parachain ID",
		props(Pallet = "Registrar")
	)]
	Reserve,
	/// Register a parachain ID with genesis state and code.
	#[strum(
		serialize = "register",
		message = "register",
		detailed_message = "Register a parachain ID with genesis state and code",
		props(Pallet = "Registrar")
	)]
	Register,
}

impl Action {
	/// Get the description of the action.
	pub fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the extrinsic name corresponding to the action.
	pub fn extrinsic_name(&self) -> &str {
		self.get_message().unwrap_or_default()
	}

	/// Get the associated pallet name for the action.
	pub fn pallet_name(&self) -> &str {
		self.get_str("Pallet").unwrap_or_default()
	}
}

/// Fetch the list of supported actions based on available pallets.
///
/// # Arguments
///
/// * `pallets`: List of pallets availables in the chain.
pub async fn supported_actions(pallets: &[Pallet]) -> Vec<Action> {
	let mut actions = Vec::new();
	for action in Action::VARIANTS.iter() {
		if find_extrinsic_by_name(pallets, action.pallet_name(), action.extrinsic_name())
			.await
			.is_ok()
		{
			actions.push(action.clone());
		}
	}
	actions
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{parse_chain_metadata, set_up_api};
	use anyhow::Result;
	use std::collections::HashMap;

	#[test]
	fn action_descriptions_are_correct() {
		let descriptions = HashMap::from([
			(Action::CreateAsset, "Create an asset"),
			(Action::MintAsset, "Mint an asset"),
			(Action::CreateCollection, "Create an NFT collection"),
			(Action::MintNFT, "Mint an NFT"),
			(Action::PurchaseOnDemandCoretime, "Purchase on-demand coretime"),
			(Action::Transfer, "Transfer balance"),
			(Action::Register, "Register a parachain ID with genesis state and code"),
			(Action::Reserve, "Reserve a parachain ID"),
		]);

		for action in Action::VARIANTS.iter() {
			assert_eq!(&action.description(), descriptions.get(action).unwrap());
		}
	}

	#[test]
	fn pallet_names_are_correct() {
		let pallets = HashMap::from([
			(Action::CreateAsset, "Assets"),
			(Action::MintAsset, "Assets"),
			(Action::CreateCollection, "Nfts"),
			(Action::MintNFT, "Nfts"),
			(Action::PurchaseOnDemandCoretime, "OnDemand"),
			(Action::Transfer, "Balances"),
			(Action::Register, "Registrar"),
			(Action::Reserve, "Registrar"),
		]);

		for action in Action::VARIANTS.iter() {
			assert_eq!(&action.pallet_name(), pallets.get(action).unwrap(),);
		}
	}

	#[test]
	fn extrinsic_names_are_correct() {
		let pallets = HashMap::from([
			(Action::CreateAsset, "create"),
			(Action::MintAsset, "mint"),
			(Action::CreateCollection, "create"),
			(Action::MintNFT, "mint"),
			(Action::PurchaseOnDemandCoretime, "place_order_allow_death"),
			(Action::Transfer, "transfer_allow_death"),
			(Action::Register, "register"),
			(Action::Reserve, "reserve"),
		]);

		for action in Action::VARIANTS.iter() {
			assert_eq!(&action.extrinsic_name(), pallets.get(action).unwrap(),);
		}
	}

	#[tokio::test]
	async fn supported_actions_works() -> Result<()> {
		// Test Pop Parachain.
		let mut api = set_up_api("wss://rpc1.paseo.popnetwork.xyz").await?;
		let mut actions = supported_actions(&parse_chain_metadata(&api).await?).await;
		assert_eq!(actions.len(), 5);
		assert_eq!(actions[0], Action::Transfer);
		assert_eq!(actions[1], Action::CreateAsset);
		assert_eq!(actions[2], Action::MintAsset);
		assert_eq!(actions[3], Action::CreateCollection);
		assert_eq!(actions[4], Action::MintNFT);

		// Test Polkadot Relay Chain.
		api = set_up_api("wss://polkadot-rpc.publicnode.com").await?;
		actions = supported_actions(&parse_chain_metadata(&api).await?).await;
		assert_eq!(actions.len(), 4);
		assert_eq!(actions[0], Action::Transfer);
		assert_eq!(actions[1], Action::PurchaseOnDemandCoretime);
		assert_eq!(actions[2], Action::Reserve);
		assert_eq!(actions[3], Action::Register);

		Ok(())
	}
}
