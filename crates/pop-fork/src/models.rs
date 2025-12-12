use crate::schema::{blocks, storage};
use diesel::{Insertable, Queryable, Selectable};

#[derive(Insertable, Clone)]
#[diesel(table_name = storage)]
pub struct StorageRow {
	pub block_hash: Vec<u8>,
	pub key: Vec<u8>,
	pub value: Option<Vec<u8>>,
	pub is_empty: bool,
}

#[derive(Insertable, Queryable, Selectable, Clone)]
#[diesel(table_name = blocks)]
pub struct BlockRow {
	pub hash: Vec<u8>,
	pub number: i64,
	pub parent_hash: Vec<u8>,
	pub header: Vec<u8>,
}
