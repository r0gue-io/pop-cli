use crate::schema::{blocks, local_keys, local_values, prefix_scans, storage};
use diesel::{Insertable, Queryable, Selectable};

#[derive(Insertable, Clone)]
#[diesel(table_name = storage)]
pub(crate) struct NewStorageRow<'a> {
	pub block_hash: &'a [u8],
	pub key: &'a [u8],
	pub value: Option<&'a [u8]>,
	pub is_empty: bool,
}

/// Local key row for insertions
#[derive(Insertable, Clone)]
#[diesel(table_name = local_keys)]
pub(crate) struct NewLocalKeyRow<'a> {
	pub key: &'a [u8],
}

/// Local key row for query results
#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = local_keys)]
pub(crate) struct LocalKeyRow {
	pub id: i32,
	pub key: Vec<u8>,
}

/// Local value row for insertions
#[derive(Insertable, Clone)]
#[diesel(table_name = local_values)]
pub(crate) struct NewLocalValueRow {
	pub key_id: i32,
	pub value: Vec<u8>,
	pub valid_from: i64,
	pub valid_until: Option<i64>,
}

/// Local value row for query results
#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = local_values)]
pub(crate) struct LocalValueRow {
	pub id: i32,
	pub key_id: i32,
	pub value: Vec<u8>,
	pub valid_from: i64,
	pub valid_until: Option<i64>,
}

/// Block row for insertions (uses borrowed data to avoid allocations)
#[derive(Insertable, Clone)]
#[diesel(table_name = blocks)]
pub(crate) struct NewBlockRow<'a> {
	pub hash: &'a [u8],
	pub number: i64,
	pub parent_hash: &'a [u8],
	pub header: &'a [u8],
}

/// Block row for query results (uses owned data)
#[derive(Queryable, Selectable, Clone, Debug)]
#[diesel(table_name = blocks)]
pub struct BlockRow {
	pub hash: Vec<u8>,
	pub number: i64,
	pub parent_hash: Vec<u8>,
	pub header: Vec<u8>,
}

/// Prefix scan row for insertions (uses borrowed data to avoid allocations)
#[derive(Insertable, Clone)]
#[diesel(table_name = prefix_scans)]
pub(crate) struct NewPrefixScanRow<'a> {
	pub(crate) block_hash: &'a [u8],
	pub(crate) prefix: &'a [u8],
	pub(crate) last_scanned_key: Option<&'a [u8]>,
	pub(crate) is_complete: bool,
}
