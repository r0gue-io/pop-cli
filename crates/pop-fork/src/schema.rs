// SPDX-License-Identifier: GPL-3.0

// Diesel schema for SQLite tables used by the cache layer.
diesel::table! {
	storage (block_hash, key) {
		block_hash -> Binary,
		key -> Binary,
		value -> Nullable<Binary>,
		is_empty -> Bool,
	}
}

diesel::table! {
	blocks (hash) {
		hash -> Binary,
		number -> Integer,
		parent_hash -> Binary,
		header -> Binary,
	}
}
