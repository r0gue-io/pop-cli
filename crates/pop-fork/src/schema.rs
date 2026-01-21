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
	local_keys (id) {
		id -> Integer,
		key -> Binary,
	}
}

diesel::table! {
	local_values (id) {
		id -> Integer,
		key_id -> Integer,
		value -> Binary,
		valid_from -> BigInt,
		valid_until -> Nullable<BigInt>,
	}
}

diesel::table! {
	prefix_scans (block_hash, prefix) {
		block_hash -> Binary,
		prefix -> Binary,
		last_scanned_key -> Nullable<Binary>,
		is_complete -> Bool,
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

diesel::joinable!(local_values -> local_keys (key_id));

diesel::allow_tables_to_appear_in_same_query!(
	blocks,
	local_keys,
	local_values,
	prefix_scans,
	storage,
);
