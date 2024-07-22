use strum_macros::{EnumIter, EnumMessage};

/// This enum is used to register from the CLI which types that are kind of usual in config traits are included in the pallet
#[derive(Debug, Copy, Clone, PartialEq ,EnumIter, EnumMessage)]
pub enum TemplatePalletConfigCommonTypes{
    #[strum(message="RuntimeEvent", detailed_message="This type will enable your pallet to emit events.")]
    RuntimeEvent, 
    #[strum(message="RuntimeOrigin", detailed_message="This type will be helful if your pallet needs to deal with the outer RuntimeOrigin enum, or if your pallet needs to use custom origins. If you add it, later on you'll be able to add a custom internal origin to the template.")]
    RuntimeOrigin,
    #[strum(message="Currency", detailed_message="This type will allow your pallet to interact with the native currency of the blockchain.")]
    Currency
}

/// This enum is used to determine if a pallet config type is included in the metadata (annotated with #[pallet::constant]) or not.
#[derive(Debug, Copy, Clone, EnumIter, EnumMessage)]
pub enum TemplatePalletConfigTypesMetadata {
	#[strum(message = "Not included into the metadata", detailed_message="")]
	NonConstant,
	#[strum(message = "Included into the metadata", detailed_message="")]
	Constant,
}

/// This enum is used to specify the default config options of a pallet's config type if needed
#[derive(Debug, Copy, Clone, EnumIter, EnumMessage)]
pub enum TemplatePalletConfigTypesDefault{
    #[strum(message="Yes", detailed_message="This trait item may be used as a default with the #[derive_impl(..)] macro")]
    Default,
    #[strum(message = "Yes, without trait bounds", detailed_message="Ensures the generated DefaultConfig will not have any bounds for this trait item.")]
	NoDefaultBounds,
    #[strum(message = "No", detailed_message="Ensure this trait item will not be used as a default with the #[derive_impl(..)] macro")]
	NoDefault
}

/// This enum is used to determine which storage shape has a storage item in the pallet
#[derive(Debug, Copy, Clone, EnumIter, EnumMessage)]
pub enum TemplatePalletStorageTypes {
    #[strum(message = "StorageValue", detailed_message="A storage value is a single value of a given type stored on-chain.")]
	StorageValue,
    #[strum(message="StorageMap", detailed_message="A storage map is a mapping of keys to values of a given type stored on-chain.")]
	StorageMap,
    #[strum(message="CountedStorageMap", detailed_message="A wrapper around a StorageMap and a StorageValue (with the value being u32) to keep track of how many items are in a map.")]
	CountedStorageMap,
    #[strum(message="StorageDoubleMap", detailed_message="This structure associates a pair of keys with a value of a specified type stored on-chain.")]
	StorageDoubleMap,
    #[strum(message="StorageNMap", detailed_message="This structure associates an arbitrary number of keys with a value of a specified type stored on-chain.")]
	StorageNMap,
    #[strum(message="CountedStorageNMap", detailed_message="A wrapper around a StorageNMap and a StorageValue (with the value being u32) to keep track of how many items are in a map.")]
	CountedStorageNMap,
}