use clap::ValueEnum;
use strum_macros::{EnumIter, EnumMessage};

/// This enum is used to register from the CLI which types that are kind of usual in config traits
/// are included in the pallet
#[derive(Debug, Copy, Clone, PartialEq, EnumIter, EnumMessage, ValueEnum)]
pub enum TemplatePalletConfigCommonTypes {
	/// This type will enable your pallet to emit events.
	#[strum(
		message = "RuntimeEvent",
		detailed_message = "This type will enable your pallet to emit events."
	)]
	RuntimeEvent,
	/// This type will be helpful if your pallet needs to deal with the outer RuntimeOrigin enum,
	/// or if your pallet needs to use custom origins. Note: If you have run the command using -o,
	/// this type will be added anyway.
	#[strum(
		message = "RuntimeOrigin",
		detailed_message = "This type will be helpful if your pallet needs to deal with the outer RuntimeOrigin enum, or if your pallet needs to use custom origins. Note: If you have run the command using -o, this type will be added anyway."
	)]
	RuntimeOrigin,
	/// This type will allow your pallet to interact with the native currency of the blockchain.
	#[strum(
		message = "Currency",
		detailed_message = "This type will allow your pallet to interact with the native currency of the blockchain."
	)]
	Currency,
}

/// This enum is used to determine which storage shape has a storage item in the pallet
#[derive(Debug, Copy, Clone, PartialEq, EnumIter, EnumMessage, ValueEnum)]
pub enum TemplatePalletStorageTypes {
	/// A storage value is a single value of a given type stored on-chain.
	#[strum(
		message = "StorageValue",
		detailed_message = "A storage value is a single value of a given type stored on-chain."
	)]
	StorageValue,
	/// A storage map is a mapping of keys to values of a given type stored on-chain.
	#[strum(
		message = "StorageMap",
		detailed_message = "A storage map is a mapping of keys to values of a given type stored on-chain."
	)]
	StorageMap,
	/// A wrapper around a StorageMap and a StorageValue (with the value being u32) to keep track
	/// of how many items are in a map.
	#[strum(
		message = "CountedStorageMap",
		detailed_message = "A wrapper around a StorageMap and a StorageValue (with the value being u32) to keep track of how many items are in a map."
	)]
	CountedStorageMap,
	/// This structure associates a pair of keys with a value of a specified type stored on-chain.
	#[strum(
		message = "StorageDoubleMap",
		detailed_message = "This structure associates a pair of keys with a value of a specified type stored on-chain."
	)]
	StorageDoubleMap,
	/// This structure associates an arbitrary number of keys with a value of a specified type
	/// stored on-chain.
	#[strum(
		message = "StorageNMap",
		detailed_message = "This structure associates an arbitrary number of keys with a value of a specified type stored on-chain."
	)]
	StorageNMap,
	/// A wrapper around a StorageNMap and a StorageValue (with the value being u32) to keep track
	/// of how many items are in a map.
	#[strum(
		message = "CountedStorageNMap",
		detailed_message = "A wrapper around a StorageNMap and a StorageValue (with the value being u32) to keep track of how many items are in a map."
	)]
	CountedStorageNMap,
}

/// This enum is used to register from the CLI which options are selected by the user to be included
/// in the pallet.
#[derive(Debug, Copy, Clone, PartialEq, EnumIter, EnumMessage)]
pub enum TemplatePalletOptions {
	#[strum(
		message = "DefaultConfig",
		detailed_message = "Use a default configuration for your config trait."
	)]
	DefaultConfig,
	#[strum(message = "GenesisConfig", detailed_message = "Add a genesis config to your pallet.")]
	GenesisConfig,
	#[strum(message = "Custom Origin", detailed_message = "Add a custom origin to your pallet.")]
	CustomOrigin,
}
