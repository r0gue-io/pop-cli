use crate::schema::{blocks, storage};
use diesel::{Insertable, Queryable, Selectable};

#[derive(Insertable, Clone)]
#[diesel(table_name = storage)]
pub struct NewStorageRow<'a> {
	pub block_hash: &'a [u8],
	pub key: &'a [u8],
	pub value: Option<&'a [u8]>,
	pub is_empty: bool,
}

/// Block row for insertions (uses borrowed data to avoid allocations)
#[derive(Insertable, Clone)]
#[diesel(table_name = blocks)]
pub struct NewBlockRow<'a> {
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
