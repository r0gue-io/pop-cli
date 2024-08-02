pub trait PlaceholderTraitBound: Sized {}

pub struct PlaceholderType;

impl PlaceholderTraitBound for PlaceholderType{}

#[macro_export]
macro_rules! pop_todo{
    (storage_hasher) => { Blake2_128Concat };
    (storage_key) => { u32 };
    (storage_value) => { u32 };
    (constant_default_type) => { ConstU32<1> };
    (origin_success_type) => { u32 };
    (origin_match_type) => { Ok(1) };
}