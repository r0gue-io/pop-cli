#![allow(unused)]
use syn::Ident;

/// Format containing necessary information for appending pallets
pub(super) struct AddPalletEntry {
    pub(super) index: Option<u8>,
    pub(super) path: Ident,
    pub(super) name: Ident,
}
impl AddPalletEntry {
    fn new(index: Option<u8>, path: &str, name: &str) -> Self {
        let path = Ident::new(path, proc_macro2::Span::call_site());
        let name = Ident::new(name, proc_macro2::Span::call_site());
        Self { index, path, name }
    }
}

impl From<ReadPalletEntry> for AddPalletEntry {
    fn from(value: ReadPalletEntry) -> Self {
        todo!("")
    }
}


/// All information that's needed to represent a pallet in a construct_runtime! invocation
/// The processing must be based on the context i.e. the type of RuntimeDeclaration in the runtime
pub(super) struct ReadPalletEntry {
    /// Pallet identifier. "System" in `System: frame_system = 1`
    pub(super) entry: String,
    /// Stores a tuple of information (index, instance). For single instances, instance = 0
    pub(super) numbers: Numbers,
}
#[derive(Default, Debug)]
pub(super) struct Numbers {
    /// Stores the first index as parsed from input file
    pub(super) index: Option<u8>,
    /// Counts the number of instances in runtime file
    /// 0 means only 1 unique instance was found
    /// 1 means the pallet is using instance syntax pallet::<Instance1>
    /// >1 means multiple pallet instances were found
    pub(super) instance: u8,
}