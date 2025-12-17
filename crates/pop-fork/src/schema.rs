// @generated automatically by Diesel CLI.

diesel::table! {
	blocks (hash) {
		hash -> Binary,
		number -> BigInt,
		parent_hash -> Binary,
		header -> Binary,
	}
}

diesel::table! {
	storage (block_hash, key) {
		block_hash -> Binary,
		key -> Binary,
		value -> Nullable<Binary>,
		is_empty -> Bool,
	}
}

diesel::allow_tables_to_appear_in_same_query!(blocks, storage,);
