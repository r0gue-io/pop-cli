use crate::schema::{blocks, storage};
use diesel::Insertable;

#[derive(Insertable, Clone)]
#[diesel(table_name = storage)]
pub(crate) struct NewStorageRow {
	pub(crate) block_hash: Vec<u8>,
	pub(crate) key: Vec<u8>,
	pub(crate) value: Option<Vec<u8>>,
	pub(crate) is_empty: bool,
}

#[derive(Insertable, Clone)]
#[diesel(table_name = blocks)]
pub(crate) struct NewBlockRow {
	pub(crate) hash: Vec<u8>,
	pub(crate) number: i32,
	pub(crate) parent_hash: Vec<u8>,
	pub(crate) header: Vec<u8>,
}
