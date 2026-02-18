// SPDX-License-Identifier: GPL-3.0

use super::{Pallet, find_callable_by_name};
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
	/// Create a NFT collection.
	#[strum(
		serialize = "create_nft",
		message = "create",
		detailed_message = "Create a NFT collection",
		props(Pallet = "Nfts")
	)]
	CreateCollection,
	/// Mint a NFT.
	#[strum(
		serialize = "mint_nft",
		message = "mint",
		detailed_message = "Mint a NFT",
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
	/// Create a pure proxy.
	#[strum(
		serialize = "create_pure",
		message = "create_pure",
		detailed_message = "Create a pure proxy",
		props(Pallet = "Proxy")
	)]
	PureProxy,
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
	/// Make a remark.
	#[strum(
		serialize = "remark",
		message = "remark_with_event",
		detailed_message = "Make a remark",
		props(Pallet = "System")
	)]
	Remark,
	/// Register the callers account so that it can be used in contract interactions.
	#[strum(
		serialize = "map_account",
		message = "map_account",
		detailed_message = "Map account",
		props(Pallet = "Revive")
	)]
	MapAccount,
}

impl Action {
	/// Get the description of the action.
	pub fn description(&self) -> &str {
		self.get_detailed_message().unwrap_or_default()
	}

	/// Get the dispatchable function name corresponding to the action.
	pub fn function_name(&self) -> &str {
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
/// * `pallets`: Supported pallets.
pub fn supported_actions(pallets: &[Pallet]) -> Vec<Action> {
	let mut actions = Vec::new();
	for action in Action::VARIANTS.iter() {
		if find_callable_by_name(pallets, action.pallet_name(), action.function_name()).is_ok() {
			actions.push(action.clone());
		}
	}
	actions
}

#[cfg(test)]
mod tests {
	use super::{Action::*, *};
	use crate::{parse_chain_metadata, set_up_client};
	use anyhow::Result;
	use pop_common::test_env::shared_substrate_ws_url;
	use std::collections::HashMap;

	#[test]
	fn action_descriptions_are_correct() {
		let descriptions = HashMap::from([
			(CreateAsset, "Create an asset"),
			(MintAsset, "Mint an asset"),
			(CreateCollection, "Create a NFT collection"),
			(MintNFT, "Mint a NFT"),
			(PurchaseOnDemandCoretime, "Purchase on-demand coretime"),
			(PureProxy, "Create a pure proxy"),
			(Transfer, "Transfer balance"),
			(Register, "Register a parachain ID with genesis state and code"),
			(Reserve, "Reserve a parachain ID"),
			(Remark, "Make a remark"),
			(MapAccount, "Map account"),
		]);

		for action in Action::VARIANTS.iter() {
			assert_eq!(&action.description(), descriptions.get(action).unwrap());
		}
	}

	#[test]
	fn pallet_names_are_correct() {
		let pallets = HashMap::from([
			(CreateAsset, "Assets"),
			(MintAsset, "Assets"),
			(CreateCollection, "Nfts"),
			(MintNFT, "Nfts"),
			(PurchaseOnDemandCoretime, "OnDemand"),
			(PureProxy, "Proxy"),
			(Transfer, "Balances"),
			(Register, "Registrar"),
			(Reserve, "Registrar"),
			(Remark, "System"),
			(MapAccount, "Revive"),
		]);

		for action in Action::VARIANTS.iter() {
			assert_eq!(&action.pallet_name(), pallets.get(action).unwrap(),);
		}
	}

	#[test]
	fn function_names_are_correct() {
		let pallets = HashMap::from([
			(CreateAsset, "create"),
			(MintAsset, "mint"),
			(CreateCollection, "create"),
			(MintNFT, "mint"),
			(PurchaseOnDemandCoretime, "place_order_allow_death"),
			(PureProxy, "create_pure"),
			(Transfer, "transfer_allow_death"),
			(Register, "register"),
			(Reserve, "reserve"),
			(Remark, "remark_with_event"),
			(MapAccount, "map_account"),
		]);

		for action in Action::VARIANTS.iter() {
			assert_eq!(&action.function_name(), pallets.get(action).unwrap(),);
		}
	}

	#[tokio::test]
	async fn supported_actions_works() -> Result<()> {
		let node_url = shared_substrate_ws_url().await;
		// Test Local Node.
		let client: subxt::OnlineClient<subxt::SubstrateConfig> = set_up_client(&node_url).await?;
		let pallets = parse_chain_metadata(&client)?;
		let actions = supported_actions(&pallets);
		// Kitchensink runtime includes Nfts, Proxy, and Revive pallets but not OnDemand.
		assert!(actions.contains(&Transfer));
		assert!(actions.contains(&CreateAsset));
		assert!(actions.contains(&MintAsset));
		assert!(actions.contains(&CreateCollection));
		assert!(actions.contains(&MintNFT));
		assert!(actions.contains(&PureProxy));
		assert!(actions.contains(&Remark));
		assert!(actions.contains(&MapAccount));
		assert!(!actions.contains(&PurchaseOnDemandCoretime));
		Ok(())
	}
}
