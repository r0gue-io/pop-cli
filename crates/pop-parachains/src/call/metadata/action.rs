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
	PartialEq,
	VariantArray,
)]
pub enum Action {
	#[strum(
		serialize = "transfer",
		message = "transfer_allow_death",
		detailed_message = "Transfer Balance",
		props(Pallet = "Balances")
	)]
	Transfer,
	#[strum(
		serialize = "create",
		message = "create",
		detailed_message = "Create an Asset",
		props(Pallet = "Assets")
	)]
	CreateAsset,
	#[strum(
		serialize = "mint",
		message = "mint",
		detailed_message = "Mint an Asset",
		props(Pallet = "Assets")
	)]
	MintAsset,
	#[strum(
		serialize = "create_nft",
		message = "create",
		detailed_message = "Create an NFT Collection",
		props(Pallet = "Nfts")
	)]
	CreateCollection,
	#[strum(
		serialize = "mint_nft",
		message = "mint",
		detailed_message = "Mint an NFT",
		props(Pallet = "Nfts")
	)]
	MintNFT,
	#[strum(
		serialize = "place_order_allow_death",
		message = "place_order_allow_death",
		detailed_message = "Purchase on-demand coretime",
		props(Pallet = "OnDemand")
	)]
	PurchaseOnDemandCoretime,
	#[strum(
		serialize = "reserve",
		message = "reserve",
		detailed_message = "Reserve para id",
		props(Pallet = "Registrar")
	)]
	Reserve,
	#[strum(
		serialize = "register",
		message = "register",
		detailed_message = "Register para id with genesis state and code",
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
