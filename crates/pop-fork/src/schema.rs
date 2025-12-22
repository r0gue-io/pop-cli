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
	local_storage (block_number, key) {
		block_number -> BigInt,
		key -> Binary,
		value -> Nullable<Binary>,
		is_empty -> Bool,
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

diesel::allow_tables_to_appear_in_same_query!(blocks, storage, local_storage);

diesel::table! {
	prefix_scans (block_hash, prefix) {
		block_hash -> Binary,
		prefix -> Binary,
		last_scanned_key -> Nullable<Binary>,
		is_complete -> Bool,
	}
}
