// SPDX-License-Identifier: GPL-3.0

//! SQLite-based storage cache for fork operations.
//!
//! Provides persistent caching of storage values fetched from live chains,
//! enabling fast restarts and reducing RPC calls.

use crate::{
	error::cache::CacheError,
	models::{
		BlockRow, LocalKeyRow, NewBlockRow, NewLocalKeyRow, NewLocalValueRow, NewPrefixScanRow,
		NewStorageRow,
	},
	schema::{blocks, local_keys, local_values, prefix_scans, storage},
	strings::cache::{errors, lock_patterns, pragmas, urls},
};
use bb8::CustomizeConnection;
use diesel::{
	OptionalExtension, prelude::*, result::Error as DieselError, sqlite::SqliteConnection,
};
use diesel_async::{
	AsyncConnection, AsyncMigrationHarness, RunQueryDsl,
	pooled_connection::{
		AsyncDieselConnectionManager, PoolError,
		bb8::{Pool, PooledConnection},
	},
	sync_connection_wrapper::SyncConnectionWrapper,
};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::{
	collections::{HashMap, HashSet},
	future::Future,
	ops::{Deref, DerefMut},
	path::Path,
	pin::Pin,
	sync::Arc,
	time::Duration,
};
use subxt::config::substrate::H256;
use tokio::sync::{Mutex, MutexGuard};

/// Maximum number of connections in the SQLite connection pool.
///
/// Since Pop is the only process accessing the database, this is for internal
/// async task concurrency. 5 provides comfortable headroom for parallel operations
/// while remaining lightweight on end-user devices.
const MAX_POOL_CONNECTIONS: u32 = 5;
/// Maximum retries for transient SQLite lock/busy errors on write paths.
const MAX_LOCK_RETRIES: u32 = 30;

/// Progress information for a prefix scan operation.
///
/// Tracks the state of an incremental prefix scan, enabling resumable
/// operations that can be interrupted and continued later.
///
/// # Lifecycle
///
/// 1. **Not started**: `get_prefix_scan_progress()` returns `None`
/// 2. **In progress**: `is_complete = false`, `last_scanned_key` holds the resume point
/// 3. **Completed**: `is_complete = true`, all keys for the prefix have been scanned
#[derive(Debug, Clone)]
pub struct PrefixScanProgress {
	/// The last key that was successfully scanned.
	/// Used as the starting point when resuming an interrupted scan.
	pub last_scanned_key: Option<Vec<u8>>,
	/// Whether the scan has processed all keys matching the prefix.
	pub is_complete: bool,
}

/// SQLite-backed persistent cache for storage values.
///
/// Enables fast restarts without re-fetching all data from live chains
/// and reduces load on public RPC endpoints.
#[derive(Clone, Debug)]
pub struct StorageCache {
	inner: StorageConn,
}

/// Internal connection wrapper for the storage cache.
#[derive(Clone)]
enum StorageConn {
	/// For file-based databases, uses a connection pool to enable concurrent access
	/// from multiple async tasks. This is more efficient for persistent storage where multiple
	/// operations may run in parallel.
	Pool(Pool<SyncConnectionWrapper<SqliteConnection>>),
	/// For in-memory databases, uses a single shared connection protected by a mutex.
	/// In-memory databases don't benefit from connection pools since all connections share the
	/// same memory state.
	Single(Arc<Mutex<SyncConnectionWrapper<SqliteConnection>>>),
}

impl std::fmt::Debug for StorageConn {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			StorageConn::Pool(_) => f.debug_tuple("Pool").field(&"...").finish(),
			StorageConn::Single(_) => f.debug_tuple("Single").field(&"...").finish(),
		}
	}
}

/// Connection guard that handles both pool and single connection types.
///
/// Automatically returns the connection to the pool or unlocks the mutex when dropped.
pub(crate) enum ConnectionGuard<'a> {
	Pool(PooledConnection<'a, SyncConnectionWrapper<SqliteConnection>>),
	Single(MutexGuard<'a, SyncConnectionWrapper<SqliteConnection>>),
}

impl<'a> Deref for ConnectionGuard<'a> {
	type Target = SyncConnectionWrapper<SqliteConnection>;

	fn deref(&self) -> &Self::Target {
		match self {
			ConnectionGuard::Pool(conn) => conn,
			ConnectionGuard::Single(guard) => guard,
		}
	}
}

impl<'a> DerefMut for ConnectionGuard<'a> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		match self {
			ConnectionGuard::Pool(conn) => conn,
			ConnectionGuard::Single(guard) => guard,
		}
	}
}

/// Increments the attempt counter and sleeps with linear backoff.
///
/// Uses a simple linear backoff strategy: delay = 10ms * attempt_number.
/// This gives the database time to release locks while avoiding excessive delays.
async fn retry_conn(attempts: &mut u32) {
	*attempts += 1;
	let delay_ms = 10u64.saturating_mul(*attempts as u64);
	tokio::time::sleep(Duration::from_millis(delay_ms)).await;
}

/// Connection customizer that sets SQLite pragmas on each pooled connection.
#[derive(Debug, Clone, Copy)]
struct SqliteConnectionCustomizer;

impl CustomizeConnection<SyncConnectionWrapper<SqliteConnection>, PoolError>
	for SqliteConnectionCustomizer
{
	fn on_acquire<'a>(
		&'a self,
		conn: &'a mut SyncConnectionWrapper<SqliteConnection>,
	) -> Pin<Box<dyn Future<Output = Result<(), PoolError>> + Send + 'a>> {
		Box::pin(async move {
			// Set busy timeout to reduce lock errors under contention
			diesel::sql_query(pragmas::BUSY_TIMEOUT)
				.execute(conn)
				.await
				.map_err(PoolError::QueryError)?;
			Ok(())
		})
	}
}

impl StorageCache {
	/// Open or create a cache database at the specified path.
	///
	/// Creates the parent directory if it doesn't exist.
	pub async fn open(maybe_path: Option<&Path>) -> Result<Self, CacheError> {
		// For in-memory open a single dedicated connection; for file path use a pool.
		if let Some(path) = maybe_path {
			// Ensure parent directory exists
			if let Some(parent) = path.parent() {
				std::fs::create_dir_all(parent)?;
			}
			let url = path.display().to_string();

			// Run migrations on a temporary async connection first
			{
				let mut conn = SyncConnectionWrapper::<SqliteConnection>::establish(&url).await?;
				// Apply pragmas for better concurrency on file databases
				// WAL mode: Persists to the database file itself
				diesel::sql_query(pragmas::JOURNAL_MODE_WAL).execute(&mut conn).await?;
				// Busy timeout: For this migration connection
				diesel::sql_query(pragmas::BUSY_TIMEOUT).execute(&mut conn).await?;
				let mut harness = AsyncMigrationHarness::new(conn);
				harness.run_pending_migrations(MIGRATIONS)?;
				let _ = harness.into_inner();
			}

			// Build the pool with connection customizer
			let manager =
				AsyncDieselConnectionManager::<SyncConnectionWrapper<SqliteConnection>>::new(url);
			let pool = Pool::builder()
				.max_size(MAX_POOL_CONNECTIONS)
				.connection_customizer(Box::new(SqliteConnectionCustomizer))
				.build(manager)
				.await?;
			Ok(Self { inner: StorageConn::Pool(pool) })
		} else {
			// Single in-memory connection
			let mut conn =
				SyncConnectionWrapper::<SqliteConnection>::establish(urls::IN_MEMORY).await?;
			// Run migrations on this single connection
			// Set busy timeout to reduce lock errors under contention
			diesel::sql_query(pragmas::BUSY_TIMEOUT).execute(&mut conn).await?;
			let mut harness = AsyncMigrationHarness::new(conn);
			harness.run_pending_migrations(MIGRATIONS)?;
			let conn = harness.into_inner();
			Ok(Self {
				inner: StorageConn::Single(std::sync::Arc::new(tokio::sync::Mutex::new(conn))),
			})
		}
	}

	/// Open an in-memory cache.
	///
	/// Creates a fresh in-memory SQLite database and runs all migrations
	/// to set up the storage and blocks tables.
	pub async fn in_memory() -> Result<Self, CacheError> {
		Self::open(None).await
	}

	/// Get a database connection.
	///
	/// Handles acquiring the connection from either the pool or single mutex.
	/// The connection is automatically returned to the pool or unlocks the mutex when dropped.
	pub(crate) async fn get_conn(&self) -> Result<ConnectionGuard<'_>, CacheError> {
		match &self.inner {
			StorageConn::Pool(pool) => {
				let conn = pool.get().await.map_err(|e| {
					CacheError::Connection(ConnectionError::BadConnection(e.to_string()))
				})?;
				Ok(ConnectionGuard::Pool(conn))
			},
			StorageConn::Single(m) => {
				let conn = m.lock().await;
				Ok(ConnectionGuard::Single(conn))
			},
		}
	}

	/// Get a cached storage value.
	///
	/// # Returns
	/// * `Ok(Some(Some(value)))` - Cached with a value.
	/// * `Ok(Some(None))` - Cached as empty (storage key exists but has no value).
	/// * `Ok(None)` - Not in cache (unknown).
	pub async fn get_storage(
		&self,
		block_hash: H256,
		key: &[u8],
	) -> Result<Option<Option<Vec<u8>>>, CacheError> {
		// Retrieve the cached value and its empty flag for the given block and key.
		// We need both `value` and `is_empty` to distinguish between:
		// - Key not in cache (no row returned)
		// - Key cached as empty (row exists, is_empty = true)
		// - Key cached with value (row exists, is_empty = false)
		use crate::schema::storage::columns as sc;

		let mut conn = self.get_conn().await?;

		let row: Option<(Option<Vec<u8>>, bool)> = storage::table
			.filter(sc::block_hash.eq(block_hash.as_bytes()))
			.filter(sc::key.eq(key))
			.select((sc::value, sc::is_empty))
			.first::<(Option<Vec<u8>>, bool)>(&mut conn)
			.await
			.optional()?;

		Ok(row.map(|(val, empty)| if empty { None } else { val }))
	}

	/// Cache a storage value.
	///
	/// # Arguments
	/// * `block_hash` - The block hash this storage is from
	/// * `key` - The storage key
	/// * `value` - The storage value, or None if the key has no value (empty)
	pub async fn set_storage(
		&self,
		block_hash: H256,
		key: &[u8],
		value: Option<&[u8]>,
	) -> Result<(), CacheError> {
		// Insert or update the cached storage entry with simple retry on lock contention.
		use crate::schema::storage::columns as sc;

		// Retry loop for transient SQLite lock/busy errors.
		// SQLite may return SQLITE_BUSY when another connection holds a lock.
		// We retry up to MAX_LOCK_RETRIES times with increasing backoff delays.
		let mut attempts = 0;
		loop {
			let mut conn = self.get_conn().await?;

			let row = NewStorageRow {
				block_hash: block_hash.as_bytes(),
				key,
				value,
				is_empty: value.is_none(),
			};

			let res = diesel::insert_into(storage::table)
				.values(&row)
				.on_conflict((sc::block_hash, sc::key))
				.do_update()
				.set((sc::value.eq(value), sc::is_empty.eq(row.is_empty)))
				.execute(&mut conn)
				.await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Get multiple cached storage values in a batch.
	///
	/// Returns results in the same order as the input keys.
	pub async fn get_storage_batch(
		&self,
		block_hash: H256,
		keys: &[&[u8]],
	) -> Result<Vec<Option<Option<Vec<u8>>>>, CacheError> {
		if keys.is_empty() {
			return Ok(vec![]);
		}

		let mut seen = HashSet::with_capacity(keys.len());
		if keys.iter().any(|key| !seen.insert(key)) {
			return Err(CacheError::DuplicatedKeys);
		}

		use crate::schema::storage::columns as sc;
		let mut conn = self.get_conn().await?;

		let rows: Vec<(Vec<u8>, Option<Vec<u8>>, bool)> = storage::table
			.filter(sc::block_hash.eq(block_hash.as_bytes()))
			.filter(sc::key.eq_any(keys))
			.select((sc::key, sc::value, sc::is_empty))
			.load::<(Vec<u8>, Option<Vec<u8>>, bool)>(&mut conn)
			.await?;

		// Build a map from the results. SQLite doesn't guarantee result order matches
		// the IN clause order, so we use a HashMap to look up values by key.
		let mut cache_map = HashMap::new();
		for (key, value, empty) in rows {
			let value = if empty { None } else { value };
			cache_map.insert(key, value);
		}

		// Return values in the same order as input keys.
		// Keys not found in cache_map (not in DB) return None (not cached).
		// Keys found return Some(value) where value is None for empty or Some(bytes) for data.
		Ok(keys.iter().map(|key| cache_map.remove(*key)).collect())
	}

	/// Cache multiple storage values in a batch.
	///
	/// Uses a transaction for efficiency.
	pub async fn set_storage_batch(
		&self,
		block_hash: H256,
		entries: &[(&[u8], Option<&[u8]>)],
	) -> Result<(), CacheError> {
		if entries.is_empty() {
			return Ok(());
		}

		let mut seen = HashSet::with_capacity(entries.len());
		if entries.iter().any(|(key, _)| !seen.insert(key)) {
			return Err(CacheError::DuplicatedKeys);
		}

		// Use a transaction to batch all inserts together.
		// This is significantly faster than individual inserts because:
		// 1. SQLite commits are expensive (fsync to disk)
		// 2. A transaction groups all inserts into a single commit
		// 3. If any insert fails, the entire batch is rolled back
		use crate::schema::storage::columns as sc;
		let entries = Arc::new(entries);
		let block_hash = Arc::new(block_hash);

		// Retry loop for transient SQLite lock/busy errors.
		// SQLite may return SQLITE_BUSY when another connection holds a lock.
		// We retry up to MAX_LOCK_RETRIES (30) times with increasing backoff delays.
		let mut attempts = 0;
		loop {
			let entries = Arc::clone(&entries);
			let block_hash = Arc::clone(&block_hash);
			let mut conn = self.get_conn().await?;
			let res = conn
				.transaction::<_, DieselError, _>(move |conn| {
					Box::pin(async move {
						let new_rows: Vec<NewStorageRow> = entries
							.iter()
							.map(|(key, value)| NewStorageRow {
								block_hash: block_hash.as_bytes(),
								key,
								value: *value,
								is_empty: value.is_none(),
							})
							.collect();
						for row in new_rows {
							diesel::insert_into(storage::table)
								.values(&row)
								.on_conflict((sc::block_hash, sc::key))
								.do_update()
								.set((sc::value.eq(row.value), sc::is_empty.eq(row.is_empty)))
								.execute(conn)
								.await?;
						}
						Ok(())
					})
				})
				.await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Get a local key's ID from the local_keys table.
	///
	/// # Returns
	/// * `Ok(Some(key_row))` - Key exists with its ID
	/// * `Ok(None)` - Key not in local_keys table
	pub async fn get_local_key(&self, key: &[u8]) -> Result<Option<LocalKeyRow>, CacheError> {
		use crate::schema::local_keys::columns as lkc;

		let mut conn = self.get_conn().await?;

		let row = local_keys::table
			.filter(lkc::key.eq(key))
			.select(LocalKeyRow::as_select())
			.first(&mut conn)
			.await
			.optional()?;

		Ok(row)
	}

	/// Insert a new key into local_keys and return its ID.
	///
	/// If the key already exists, returns the existing ID.
	pub async fn insert_local_key(&self, key: &[u8]) -> Result<i32, CacheError> {
		use crate::schema::local_keys::columns as lkc;

		let mut attempts = 0;
		loop {
			let mut conn = self.get_conn().await?;

			// Try to insert, ignore conflict (key already exists)
			let res = diesel::insert_into(local_keys::table)
				.values(NewLocalKeyRow { key })
				.on_conflict(lkc::key)
				.do_nothing()
				.execute(&mut conn)
				.await;

			match res {
				Ok(_) => {
					// Fetch the ID (either just inserted or already existed)
					let key_id: i32 = local_keys::table
						.filter(lkc::key.eq(key))
						.select(lkc::id)
						.first(&mut conn)
						.await?;
					return Ok(key_id);
				},
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Get a local storage value valid at a specific block number.
	///
	/// Queries the local_values table for a value that is valid at the given block:
	/// valid_from <= block_number AND (valid_until IS NULL OR valid_until > block_number)
	///
	/// # Returns
	/// * `Ok(Some(Some(value)))` - Value found with data
	/// * `Ok(Some(None))` - Value found but explicitly deleted (NULL in DB)
	/// * `Ok(None)` - No value valid at this block (no row found)
	pub async fn get_local_value_at_block(
		&self,
		key: &[u8],
		block_number: u32,
	) -> Result<Option<Option<Vec<u8>>>, CacheError> {
		use crate::schema::{local_keys::columns as lkc, local_values::columns as lvc};

		let mut conn = self.get_conn().await?;
		let block_num = block_number as i64;

		// First get the key_id
		let key_id: i32 = match local_keys::table
			.filter(lkc::key.eq(key))
			.select(lkc::id)
			.first(&mut conn)
			.await
			.optional()?
		{
			Some(id) => id,
			_ => return Ok(None),
		};

		// Query for value valid at block_number (value can be NULL for deletions)
		let value: Option<Option<Vec<u8>>> = local_values::table
			.filter(lvc::key_id.eq(key_id))
			.filter(lvc::valid_from.le(block_num))
			.filter(lvc::valid_until.is_null().or(lvc::valid_until.gt(block_num)))
			.select(lvc::value)
			.first(&mut conn)
			.await
			.optional()?;

		Ok(value)
	}

	/// Get multiple local storage values valid at a specific block number.
	///
	/// Returns results in the same order as the input keys.
	/// * `Some(Some(value))` - Value found with data
	/// * `Some(None)` - Value found but explicitly deleted (NULL in DB)
	/// * `None` - No value valid at this block (no row found)
	pub async fn get_local_values_at_block_batch(
		&self,
		keys: &[&[u8]],
		block_number: u32,
	) -> Result<Vec<Option<Option<Vec<u8>>>>, CacheError> {
		if keys.is_empty() {
			return Ok(vec![]);
		}

		let mut seen = HashSet::with_capacity(keys.len());
		if keys.iter().any(|key| !seen.insert(key)) {
			return Err(CacheError::DuplicatedKeys);
		}

		use crate::schema::{local_keys::columns as lkc, local_values::columns as lvc};

		let mut conn = self.get_conn().await?;
		let block_num = block_number as i64;

		// Get all key_ids for the requested keys
		let key_rows: Vec<LocalKeyRow> = local_keys::table
			.filter(lkc::key.eq_any(keys))
			.select(LocalKeyRow::as_select())
			.load(&mut conn)
			.await?;

		// Build a map from key bytes to key_id
		let key_to_id: HashMap<Vec<u8>, i32> =
			key_rows.iter().map(|r| (r.key.clone(), r.id)).collect();

		// Get all key_ids that exist
		let key_ids: Vec<i32> = key_to_id.values().copied().collect();

		if key_ids.is_empty() {
			return Ok(vec![None; keys.len()]);
		}

		// Query for values valid at block_number for all key_ids (value can be NULL for deletions)
		let value_rows: Vec<(i32, Option<Vec<u8>>)> = local_values::table
			.filter(lvc::key_id.eq_any(&key_ids))
			.filter(lvc::valid_from.le(block_num))
			.filter(lvc::valid_until.is_null().or(lvc::valid_until.gt(block_num)))
			.select((lvc::key_id, lvc::value))
			.load(&mut conn)
			.await?;

		// Build a map from key_id to value (Option<Vec<u8>> to track deletions)
		let mut id_to_value: HashMap<i32, Option<Vec<u8>>> = HashMap::new();
		for (key_id, value) in value_rows {
			id_to_value.insert(key_id, value);
		}

		// Build result in same order as input keys
		// None = key not found in local storage
		// Some(None) = key was deleted
		// Some(Some(value)) = key has a value
		Ok(keys
			.iter()
			.map(|key| key_to_id.get(*key).and_then(|key_id| id_to_value.remove(key_id)))
			.collect())
	}

	/// Get all locally-modified keys matching a prefix that existed at a specific block.
	///
	/// Joins `local_keys` with `local_values` to find keys where:
	/// - The key starts with `prefix`
	/// - A value entry is valid at `block_number` (valid_from <= block AND (valid_until IS NULL OR
	///   valid_until > block))
	/// - The value is not NULL (i.e., the key was not deleted at that block)
	///
	/// Returns a sorted list of matching keys.
	pub async fn get_local_keys_at_block(
		&self,
		prefix: &[u8],
		block_number: u32,
	) -> Result<Vec<Vec<u8>>, CacheError> {
		use crate::schema::{local_keys::columns as lkc, local_values::columns as lvc};

		let mut conn = self.get_conn().await?;
		let block_num = block_number as i64;
		let prefix_vec = prefix.to_vec();

		let mut query = local_keys::table
			.inner_join(local_values::table)
			.filter(lkc::key.ge(&prefix_vec))
			.filter(lvc::valid_from.le(block_num))
			.filter(lvc::valid_until.is_null().or(lvc::valid_until.gt(block_num)))
			.filter(lvc::value.is_not_null())
			.select(lkc::key)
			.distinct()
			.order(lkc::key.asc())
			.into_boxed();

		if let Some(upper) = Self::prefix_upper_bound(prefix) {
			query = query.filter(lkc::key.lt(upper));
		}

		let keys: Vec<Vec<u8>> = query.load(&mut conn).await?;
		Ok(keys)
	}

	/// Get all locally-deleted keys matching a prefix at a specific block.
	///
	/// Returns keys where the value entry valid at `block_number` has a NULL value
	/// (explicitly deleted). This is needed to exclude deleted keys from merged
	/// remote + local key enumeration.
	pub async fn get_local_deleted_keys_at_block(
		&self,
		prefix: &[u8],
		block_number: u32,
	) -> Result<Vec<Vec<u8>>, CacheError> {
		use crate::schema::{local_keys::columns as lkc, local_values::columns as lvc};

		let mut conn = self.get_conn().await?;
		let block_num = block_number as i64;
		let prefix_vec = prefix.to_vec();

		let mut query = local_keys::table
			.inner_join(local_values::table)
			.filter(lkc::key.ge(&prefix_vec))
			.filter(lvc::valid_from.le(block_num))
			.filter(lvc::valid_until.is_null().or(lvc::valid_until.gt(block_num)))
			.filter(lvc::value.is_null())
			.select(lkc::key)
			.distinct()
			.order(lkc::key.asc())
			.into_boxed();

		if let Some(upper) = Self::prefix_upper_bound(prefix) {
			query = query.filter(lkc::key.lt(upper));
		}

		let keys: Vec<Vec<u8>> = query.load(&mut conn).await?;
		Ok(keys)
	}

	/// Compute the exclusive upper bound for a binary prefix range query.
	///
	/// Increments the prefix to produce the first byte sequence that does NOT
	/// start with `prefix`. Returns `None` if the prefix is all `0xFF` bytes
	/// (no upper bound needed, `>=` is sufficient).
	fn prefix_upper_bound(prefix: &[u8]) -> Option<Vec<u8>> {
		let mut upper = prefix.to_vec();
		// Walk backwards, incrementing the last non-0xFF byte.
		while let Some(last) = upper.last_mut() {
			if *last < 0xFF {
				*last += 1;
				return Some(upper);
			}
			upper.pop();
		}
		// All bytes were 0xFF, no upper bound possible.
		None
	}

	/// Insert a new local value entry.
	///
	/// # Arguments
	/// * `key_id` - The key ID from local_keys table
	/// * `value` - The value bytes, or None to record a deletion
	/// * `valid_from` - Block number when this value becomes valid
	pub async fn insert_local_value(
		&self,
		key_id: i32,
		value: Option<&[u8]>,
		valid_from: u32,
	) -> Result<(), CacheError> {
		let mut attempts = 0;
		loop {
			let mut conn = self.get_conn().await?;

			let row = NewLocalValueRow {
				key_id,
				value: value.map(|v| v.to_vec()),
				valid_from: valid_from as i64,
				valid_until: None,
			};

			let res =
				diesel::insert_into(local_values::table).values(&row).execute(&mut conn).await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Close the currently open local value entry (set valid_until).
	///
	/// Finds the entry for this key_id where valid_until IS NULL and sets it.
	pub async fn close_local_value(&self, key_id: i32, valid_until: u32) -> Result<(), CacheError> {
		use crate::schema::local_values::columns as lvc;

		let mut attempts = 0;
		loop {
			let mut conn = self.get_conn().await?;

			let res = diesel::update(
				local_values::table
					.filter(lvc::key_id.eq(key_id))
					.filter(lvc::valid_until.is_null()),
			)
			.set(lvc::valid_until.eq(Some(valid_until as i64)))
			.execute(&mut conn)
			.await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Commit a batch of local storage changes in a single transaction.
	///
	/// For each entry: upserts the key into `local_keys`, closes the previous value
	/// (sets `valid_until`), and inserts the new value. Wrapping everything in one
	/// transaction avoids per-operation fsync overhead, reducing commit time from
	/// tens of seconds to sub-second.
	pub async fn commit_local_changes(
		&self,
		entries: &[(&[u8], Option<&[u8]>)],
		block_number: u32,
	) -> Result<(), CacheError> {
		use crate::schema::{local_keys::columns as lkc, local_values::columns as lvc};

		if entries.is_empty() {
			return Ok(());
		}

		// Clone entries into owned data so they can move into the async closure.
		let owned: Vec<(Vec<u8>, Option<Vec<u8>>)> =
			entries.iter().map(|(k, v)| (k.to_vec(), v.map(|val| val.to_vec()))).collect();

		let mut attempts = 0;
		loop {
			let owned = owned.clone();
			let mut conn = self.get_conn().await?;

			let res = conn
				.transaction::<_, DieselError, _>(move |conn| {
					Box::pin(async move {
						for (key, value) in &owned {
							// Upsert key
							diesel::insert_into(local_keys::table)
								.values(NewLocalKeyRow { key })
								.on_conflict(lkc::key)
								.do_nothing()
								.execute(conn)
								.await?;

							let key_id: i32 = local_keys::table
								.filter(lkc::key.eq(key.as_slice()))
								.select(lkc::id)
								.first(conn)
								.await?;

							// Close previous value
							diesel::update(
								local_values::table
									.filter(lvc::key_id.eq(key_id))
									.filter(lvc::valid_until.is_null()),
							)
							.set(lvc::valid_until.eq(Some(block_number as i64)))
							.execute(conn)
							.await?;

							// Insert new value
							let row = NewLocalValueRow {
								key_id,
								value: value.clone(),
								valid_from: block_number as i64,
								valid_until: None,
							};
							diesel::insert_into(local_values::table)
								.values(&row)
								.execute(conn)
								.await?;
						}
						Ok(())
					})
				})
				.await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Clear all local storage data (both local_keys and local_values tables).
	///
	/// This removes all locally tracked key-value pairs and their validity history.
	/// Uses a transaction to ensure both tables are cleared atomically.
	pub async fn clear_local_storage(&self) -> Result<(), CacheError> {
		let mut attempts = 0;
		loop {
			let mut conn = self.get_conn().await?;

			let res = conn
				.transaction::<_, DieselError, _>(|conn| {
					Box::pin(async move {
						// Delete local_values first (has foreign key to local_keys)
						diesel::delete(local_values::table).execute(conn).await?;
						// Then delete local_keys
						diesel::delete(local_keys::table).execute(conn).await?;
						Ok(())
					})
				})
				.await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Cache block metadata.
	pub async fn cache_block(
		&self,
		hash: H256,
		number: u32,
		parent_hash: H256,
		header: &[u8],
	) -> Result<(), CacheError> {
		// Store block metadata for quick lookup without hitting the remote RPC.
		use crate::schema::blocks::columns as bc;

		// Retry loop for transient SQLite lock/busy errors.
		// SQLite may return SQLITE_BUSY when another connection holds a lock.
		// We retry up to MAX_LOCK_RETRIES (30) times with increasing backoff delays.
		let mut attempts = 0;
		let parent_hash_bytes = parent_hash.as_bytes();
		loop {
			let mut conn = self.get_conn().await?;

			let block = NewBlockRow {
				hash: hash.as_bytes(),
				number: number as i64,
				parent_hash: parent_hash_bytes,
				header,
			};

			let res = diesel::insert_into(blocks::table)
				.values(&block)
				.on_conflict(bc::hash)
				.do_update()
				.set((
					bc::number.eq(number as i64),
					bc::parent_hash.eq(parent_hash_bytes),
					bc::header.eq(header),
				))
				.execute(&mut conn)
				.await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Get cached block metadata.
	pub async fn get_block(&self, hash: H256) -> Result<Option<BlockRow>, CacheError> {
		// Retrieve all block metadata fields by the block's hash (primary key).
		// Returns None if the block hasn't been cached yet.
		use crate::schema::blocks::columns as bc;

		let mut conn = self.get_conn().await?;

		let row = blocks::table
			.filter(bc::hash.eq(hash.as_bytes()))
			.select(BlockRow::as_select())
			.first(&mut conn)
			.await
			.optional()?;

		match row {
			// Sanity check on the block number, as we use i64 to represent them in SQLite but
			// Substrate blocks are u32
			Some(BlockRow { number, .. }) if number < 0 || number > u32::MAX.into() =>
				Err(CacheError::DataCorruption(errors::BLOCK_NUMBER_OUT_OF_U32_RANGE.into())),
			row @ Some(_) => Ok(row),
			None => Ok(None),
		}
	}

	/// Get cached block metadata by block number.
	pub async fn get_block_by_number(
		&self,
		block_number: u32,
	) -> Result<Option<BlockRow>, CacheError> {
		// Retrieve block metadata by block number.
		// Returns None if the block hasn't been cached yet.
		use crate::schema::blocks::columns as bc;

		let mut conn = self.get_conn().await?;

		let row = blocks::table
			.filter(bc::number.eq(block_number as i64))
			.select(BlockRow::as_select())
			.first(&mut conn)
			.await
			.optional()?;

		match row {
			// Sanity check on the block number
			Some(BlockRow { number, .. }) if number < 0 || number > u32::MAX.into() =>
				Err(CacheError::DataCorruption(errors::BLOCK_NUMBER_OUT_OF_U32_RANGE.into())),
			row @ Some(_) => Ok(row),
			None => Ok(None),
		}
	}

	/// Clear all cached data for a specific block.
	pub async fn clear_block(&self, hash: H256) -> Result<(), CacheError> {
		// Use a transaction to ensure both deletes succeed or fail together.
		// This maintains consistency: we never have orphaned storage entries
		// without their parent block, or vice versa.
		use crate::schema::{
			blocks::columns as bc, prefix_scans::columns as psc, storage::columns as sc,
		};
		let block_hash = Arc::new(hash.as_bytes());

		// Retry loop for transient SQLite lock/busy errors.
		// SQLite may return SQLITE_BUSY when another connection holds a lock.
		// We retry up to MAX_LOCK_RETRIES (30) times with increasing backoff delays.
		let mut attempts = 0;
		loop {
			let block_hash = Arc::clone(&block_hash);
			let mut conn = self.get_conn().await?;

			let res = conn
				.transaction::<_, DieselError, _>(move |conn| {
					Box::pin(async move {
						diesel::delete(storage::table.filter(sc::block_hash.eq(*block_hash)))
							.execute(conn)
							.await?;
						diesel::delete(blocks::table.filter(bc::hash.eq(*block_hash)))
							.execute(conn)
							.await?;
						diesel::delete(prefix_scans::table.filter(psc::block_hash.eq(*block_hash)))
							.execute(conn)
							.await?;
						Ok(())
					})
				})
				.await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Get the progress of a prefix scan operation.
	///
	/// # Returns
	/// * `Ok(Some(progress))` - Scan has been started, returns progress info
	/// * `Ok(None)` - No scan has been started for this prefix
	pub async fn get_prefix_scan_progress(
		&self,
		block_hash: H256,
		prefix: &[u8],
	) -> Result<Option<PrefixScanProgress>, CacheError> {
		use crate::schema::prefix_scans::columns as psc;

		let mut conn = self.get_conn().await?;

		let row: Option<(Option<Vec<u8>>, bool)> = prefix_scans::table
			.filter(psc::block_hash.eq(block_hash.as_bytes()))
			.filter(psc::prefix.eq(prefix))
			.select((psc::last_scanned_key, psc::is_complete))
			.first::<(Option<Vec<u8>>, bool)>(&mut conn)
			.await
			.optional()?;

		Ok(row.map(|(last_key, complete)| PrefixScanProgress {
			last_scanned_key: last_key,
			is_complete: complete,
		}))
	}

	/// Update the progress of a prefix scan operation (upsert).
	///
	/// Creates a new progress record or updates an existing one. Uses SQLite's
	/// `ON CONFLICT DO UPDATE` for atomic upsert semantics.
	///
	/// # Arguments
	/// * `block_hash` - The block hash being scanned
	/// * `prefix` - The storage prefix being scanned
	/// * `last_key` - The last key that was processed
	/// * `is_complete` - Whether the scan has finished
	pub async fn update_prefix_scan(
		&self,
		block_hash: H256,
		prefix: &[u8],
		last_key: &[u8],
		is_complete: bool,
	) -> Result<(), CacheError> {
		use crate::schema::prefix_scans::columns as psc;
		use diesel::upsert::excluded;

		let new_row = NewPrefixScanRow {
			block_hash: block_hash.as_bytes(),
			prefix,
			last_scanned_key: Some(last_key),
			is_complete,
		};

		let mut attempts = 0;
		loop {
			let mut conn = self.get_conn().await?;
			let res = diesel::insert_into(prefix_scans::table)
				.values(&new_row)
				.on_conflict((psc::block_hash, psc::prefix))
				.do_update()
				.set((
					psc::last_scanned_key.eq(excluded(psc::last_scanned_key)),
					psc::is_complete.eq(excluded(psc::is_complete)),
				))
				.execute(&mut conn)
				.await;

			match res {
				Ok(_) => return Ok(()),
				Err(e) if is_locked_error(&e) && attempts < MAX_LOCK_RETRIES => {
					retry_conn(&mut attempts).await;
					continue;
				},
				Err(e) => return Err(e.into()),
			}
		}
	}

	/// Get all cached keys matching a prefix.
	///
	/// Uses a range query (`key >= prefix AND key < prefix+1`) for efficient
	/// prefix matching on SQLite's B-tree index. This is more performant than
	/// `LIKE` or `GLOB` patterns for binary key prefixes.
	pub async fn get_keys_by_prefix(
		&self,
		block_hash: H256,
		prefix: &[u8],
	) -> Result<Vec<Vec<u8>>, CacheError> {
		use crate::schema::storage::columns as sc;

		let mut conn = self.get_conn().await?;

		// SQLite BLOB comparison with >= and < for prefix range
		let prefix_end = increment_prefix(prefix);

		let mut query = storage::table
			.filter(sc::block_hash.eq(block_hash.as_bytes()))
			.filter(sc::key.ge(prefix))
			.select(sc::key)
			.into_boxed();

		if let Some(ref end) = prefix_end {
			query = query.filter(sc::key.lt(end));
		}

		Ok(query.load::<Vec<u8>>(&mut conn).await?)
	}

	/// Find the next cached key after `key` that matches `prefix`.
	///
	/// Uses a range query (`key > current AND key >= prefix AND key < prefix+1`)
	/// for efficient lookup on SQLite's B-tree index.
	///
	/// # Returns
	/// * `Ok(Some(next_key))` - The next key after `key` matching the prefix
	/// * `Ok(None)` - No more keys with this prefix after `key`
	pub async fn next_key_from_cache(
		&self,
		block_hash: H256,
		prefix: &[u8],
		key: &[u8],
	) -> Result<Option<Vec<u8>>, CacheError> {
		use crate::schema::storage::columns as sc;

		let mut conn = self.get_conn().await?;
		let prefix_end = increment_prefix(prefix);

		let mut query = storage::table
			.filter(sc::block_hash.eq(block_hash.as_bytes()))
			.filter(sc::key.gt(key))
			.filter(sc::key.ge(prefix))
			.select(sc::key)
			.order(sc::key.asc())
			.limit(1)
			.into_boxed();

		if let Some(ref end) = prefix_end {
			query = query.filter(sc::key.lt(end));
		}

		Ok(query.first::<Vec<u8>>(&mut conn).await.optional()?)
	}

	/// Count cached keys matching a prefix.
	///
	/// Uses the same range query strategy as [`Self::get_keys_by_prefix`] for
	/// efficient counting without loading key data.
	pub async fn count_keys_by_prefix(
		&self,
		block_hash: H256,
		prefix: &[u8],
	) -> Result<usize, CacheError> {
		use crate::schema::storage::columns as sc;

		let mut conn = self.get_conn().await?;
		let prefix_end = increment_prefix(prefix);

		let mut query = storage::table
			.filter(sc::block_hash.eq(block_hash.as_bytes()))
			.filter(sc::key.ge(prefix))
			.into_boxed();

		if let Some(ref end) = prefix_end {
			query = query.filter(sc::key.lt(end));
		}

		let count: i64 = query.count().get_result(&mut conn).await?;

		Ok(count as usize)
	}
}

/// Increment a byte slice to get the exclusive upper bound for prefix queries.
/// Returns None if the prefix is all 0xFF bytes (no upper bound needed).
fn increment_prefix(prefix: &[u8]) -> Option<Vec<u8>> {
	let mut result = prefix.to_vec();
	// Find the rightmost byte that isn't 0xFF and increment it
	for i in (0..result.len()).rev() {
		if result[i] < 0xFF {
			result[i] += 1;
			result.truncate(i + 1);
			return Some(result);
		}
	}
	// All bytes were 0xFF, no upper bound
	None
}

fn is_locked_error(e: &DieselError) -> bool {
	match e {
		DieselError::DatabaseError(_, info) => {
			let msg = info.message().to_ascii_lowercase();
			msg.contains(lock_patterns::DATABASE_IS_LOCKED) || msg.contains(lock_patterns::BUSY)
		},
		_ => false,
	}
}

// Embed Diesel migrations located at `crates/pop-fork/migrations`
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test(flavor = "multi_thread")]
	async fn in_memory_cache_works() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block_hash = H256::from([1u8; 32]);
		let key = b"test_key";
		let value = b"test_value";

		// Initially not cached
		assert!(cache.get_storage(block_hash, key).await.unwrap().is_none());

		// Set a value
		cache.set_storage(block_hash, key, Some(value)).await.unwrap();

		// Now cached with value
		let cached = cache.get_storage(block_hash, key).await.unwrap();
		assert_eq!(cached, Some(Some(value.to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn cache_empty_value() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block_hash = H256::from([2u8; 32]);
		let key = b"empty_key";

		// Set as empty (key exists but no value)
		cache.set_storage(block_hash, key, None).await.unwrap();

		// Cached as empty
		let cached = cache.get_storage(block_hash, key).await.unwrap();
		assert_eq!(cached, Some(None));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn batch_operations() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block_hash = H256::from([3u8; 32]);
		let entries: Vec<(&[u8], Option<&[u8]>)> = vec![
			(b"key1", Some(b"value1")),
			(b"key2", Some(b"value2")),
			(b"key3", None), // empty
		];

		// Batch set
		cache.set_storage_batch(block_hash, &entries).await.unwrap();

		// Batch get
		let keys: Vec<&[u8]> = vec![b"key1", b"key2", b"key3", b"key4"];
		let results = cache.get_storage_batch(block_hash, &keys).await.unwrap();

		assert_eq!(results.len(), 4);
		assert_eq!(results[0], Some(Some(b"value1".to_vec())));
		assert_eq!(results[1], Some(Some(b"value2".to_vec())));
		assert_eq!(results[2], Some(None)); // empty
		assert_eq!(results[3], None); // not cached
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn block_caching() {
		let cache = StorageCache::in_memory().await.unwrap();

		let hash = H256::from([4u8; 32]);
		let parent_hash = H256::from([3u8; 32]);
		let header = b"mock_header_data";

		// Cache block
		cache.cache_block(hash, 100, parent_hash, header).await.unwrap();

		// Get block
		let block = cache.get_block(hash).await.unwrap().unwrap();
		assert_eq!(block.hash, hash.as_bytes().to_vec());
		assert_eq!(block.number, 100i64);
		assert_eq!(block.parent_hash, parent_hash.as_bytes().to_vec());
		assert_eq!(block.header, header.to_vec());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_block_with_non_cached_block() {
		let cache = StorageCache::in_memory().await.unwrap();

		let hash = H256::from([4u8; 32]);

		// Get block
		let block = cache.get_block(hash).await.unwrap();

		assert!(block.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_block_number_corrupted_block_number_fails() {
		let cache = StorageCache::in_memory().await.unwrap();

		let hash1 = H256::from([4u8; 32]);
		let hash2 = H256::from([5u8; 32]);
		let parent_hash = H256::from([3u8; 32]);
		let header = b"mock_header_data";

		// Manually insert invalid block with negative number directly into database
		let invalid_block1 = NewBlockRow {
			hash: hash1.as_bytes(),
			number: -1, // Invalid: below 0
			parent_hash: parent_hash.as_bytes(),
			header,
		};

		// Manually insert invalid block with number above the u32 maximum directly into database
		let invalid_block2 = NewBlockRow {
			hash: hash2.as_bytes(),
			number: u32::MAX as i64 + 1,
			parent_hash: parent_hash.as_bytes(),
			header,
		};

		// Insert directly into the database bypassing validation
		match &cache.inner {
			StorageConn::Single(m) => {
				let mut conn = m.lock().await;
				for block in [invalid_block1, invalid_block2] {
					diesel::insert_into(blocks::table)
						.values(&block)
						.execute(&mut *conn)
						.await
						.unwrap();
				}
			},
			_ => unreachable!("Test single connection; qed;"),
		}

		// Get block should fail with DataCorruption error
		assert!(
			matches!(cache.get_block(hash1).await, Err(CacheError::DataCorruption(msg)) if msg == errors::BLOCK_NUMBER_OUT_OF_U32_RANGE)
		);
		assert!(
			matches!(cache.get_block(hash2).await, Err(CacheError::DataCorruption(msg)) if msg == errors::BLOCK_NUMBER_OUT_OF_U32_RANGE)
		);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn different_blocks_have_separate_storage() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block1 = H256::from([5u8; 32]);
		let block2 = H256::from([6u8; 32]);
		let key = b"same_key";

		cache.set_storage(block1, key, Some(b"value1")).await.unwrap();
		cache.set_storage(block2, key, Some(b"value2")).await.unwrap();

		let cached1 = cache.get_storage(block1, key).await.unwrap();
		let cached2 = cache.get_storage(block2, key).await.unwrap();

		assert_eq!(cached1, Some(Some(b"value1".to_vec())));
		assert_eq!(cached2, Some(Some(b"value2".to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn clear_block_removes_data() {
		let cache = StorageCache::in_memory().await.unwrap();

		let hash = H256::from([7u8; 32]);
		let parent_hash = H256::from([6u8; 32]);
		let key = b"test_key";

		cache.set_storage(hash, key, Some(b"value")).await.unwrap();
		cache.cache_block(hash, 50, parent_hash, b"header").await.unwrap();

		// Data exists
		assert!(cache.get_storage(hash, key).await.unwrap().is_some());
		assert!(cache.get_block(hash).await.unwrap().is_some());

		// Clear
		cache.clear_block(hash).await.unwrap();

		// Data removed
		assert!(cache.get_storage(hash, key).await.unwrap().is_none());
		assert!(cache.get_block(hash).await.unwrap().is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn file_persistence() {
		let temp_dir = tempfile::tempdir().unwrap();
		let db_path = temp_dir.path().join("test_cache.db");

		let block_hash = H256::from([8u8; 32]);
		let key = b"persistent_key";
		let value = b"persistent_value";

		// Write and close
		{
			let cache = StorageCache::open(Some(&db_path)).await.unwrap();
			cache.set_storage(block_hash, key, Some(value)).await.unwrap();
		}

		// Reopen and verify
		{
			let cache = StorageCache::open(Some(&db_path)).await.unwrap();
			let cached = cache.get_storage(block_hash, key).await.unwrap();
			assert_eq!(cached, Some(Some(value.to_vec())));
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn concurrent_access() {
		let temp_dir = tempfile::tempdir().unwrap();
		let db_path = temp_dir.path().join("concurrent_test.db");
		let cache = StorageCache::open(Some(&db_path)).await.unwrap();

		let block_hash = H256::from([9u8; 32]);

		// Spawn multiple concurrent write tasks
		// StorageCache is cheap to clone (just increments pool's reference count)
		let mut handles = vec![];
		for i in 0..10u8 {
			let cache = cache.clone();
			let handle = tokio::spawn(async move {
				let key = format!("key_{}", i);
				let value = format!("value_{}", i);
				cache.set_storage(block_hash, key.as_bytes(), Some(value.as_bytes())).await
			});
			handles.push(handle);
		}

		// Wait for all writes to complete
		for handle in handles {
			handle.await.unwrap().unwrap();
		}

		// Spawn concurrent read tasks
		let mut read_handles = vec![];
		for i in 0..10u8 {
			let cache = cache.clone();
			let handle = tokio::spawn(async move {
				let key = format!("key_{}", i);
				cache.get_storage(block_hash, key.as_bytes()).await
			});
			read_handles.push((i, handle));
		}

		// Verify all reads return correct values
		for (i, handle) in read_handles {
			let result = handle.await.unwrap().unwrap();
			let expected_value = format!("value_{}", i);
			assert_eq!(result, Some(Some(expected_value.into_bytes())));
		}

		// Test concurrent batch operations
		let cache1 = cache.clone();
		let cache2 = cache.clone();
		let block_hash2 = H256::from([10u8; 32]);

		let batch_handle1 = tokio::spawn(async move {
			let keys: Vec<Vec<u8>> = (0..5).map(|i| format!("batch1_{}", i).into_bytes()).collect();
			let values: Vec<Vec<u8>> = (0..5).map(|i| vec![i]).collect();
			let entries: Vec<(&[u8], Option<&[u8]>)> = keys
				.iter()
				.zip(values.iter())
				.map(|(k, v)| (k.as_slice(), Some(v.as_slice())))
				.collect();
			cache1.set_storage_batch(block_hash2, &entries).await
		});

		let batch_handle2 = tokio::spawn(async move {
			let keys: Vec<Vec<u8>> =
				(5..10).map(|i| format!("batch2_{}", i).into_bytes()).collect();
			let values: Vec<Vec<u8>> = (5..10).map(|i| vec![i]).collect();
			let entries: Vec<(&[u8], Option<&[u8]>)> = keys
				.iter()
				.zip(values.iter())
				.map(|(k, v)| (k.as_slice(), Some(v.as_slice())))
				.collect();
			cache2.set_storage_batch(block_hash2, &entries).await
		});

		batch_handle1.await.unwrap().unwrap();
		batch_handle2.await.unwrap().unwrap();

		// Verify batch results
		let keys: Vec<Vec<u8>> = (0..5).map(|i| format!("batch1_{}", i).into_bytes()).collect();
		let key_refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
		let results = cache.get_storage_batch(block_hash2, &key_refs).await.unwrap();
		for (i, result) in results.iter().enumerate() {
			assert_eq!(*result, Some(Some(vec![i as u8])));
		}
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_storage_batch_with_duplicate_keys() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block_hash = H256::from([11u8; 32]);
		let entries: Vec<(&[u8], Option<&[u8]>)> = vec![
			(b"key1", Some(b"value1")),
			(b"key2", Some(b"value2")),
			(b"key3", Some(b"value3")),
		];

		// Set up some values
		cache.set_storage_batch(block_hash, &entries).await.unwrap();

		// Query with duplicate keys - key1 appears twice, key2 appears three times
		let keys: Vec<&[u8]> = vec![b"key1", b"key2", b"key1", b"key3", b"key2", b"key2"];
		let results = cache.get_storage_batch(block_hash, &keys).await;

		assert!(matches!(results, Err(CacheError::DuplicatedKeys)));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn set_storage_batch_with_duplicate_keys() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block_hash = H256::from([12u8; 32]);

		// Set batch with duplicate keys - last value should win
		let entries: Vec<(&[u8], Option<&[u8]>)> = vec![
			(b"key1", Some(b"first_value")),
			(b"key2", Some(b"value2")),
			(b"key1", Some(b"second_value")), // duplicate key1
			(b"key3", Some(b"value3")),
			(b"key1", Some(b"final_value")), // another duplicate key1
		];

		let result = cache.set_storage_batch(block_hash, &entries).await;
		assert!(matches!(result, Err(CacheError::DuplicatedKeys)));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn prefix_scan_progress_tracking() {
		let cache = StorageCache::in_memory().await.unwrap();
		let block_hash = H256::from([11u8; 32]);
		let prefix = b"balances:";

		// Initially no progress
		let progress = cache.get_prefix_scan_progress(block_hash, prefix).await.unwrap();
		assert!(progress.is_none());

		// Update progress with a partial scan
		let last_key = b"balances:account123";
		cache.update_prefix_scan(block_hash, prefix, last_key, false).await.unwrap();

		// Progress should now exist
		let progress = cache.get_prefix_scan_progress(block_hash, prefix).await.unwrap();
		assert!(progress.is_some());
		let p = progress.unwrap();
		assert_eq!(p.last_scanned_key, Some(last_key.to_vec()));
		assert!(!p.is_complete);

		// Update to complete
		let final_key = b"balances:zzz";
		cache.update_prefix_scan(block_hash, prefix, final_key, true).await.unwrap();

		let progress = cache.get_prefix_scan_progress(block_hash, prefix).await.unwrap();
		let p = progress.unwrap();
		assert_eq!(p.last_scanned_key, Some(final_key.to_vec()));
		assert!(p.is_complete);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn prefix_scan_different_blocks_separate() {
		let cache = StorageCache::in_memory().await.unwrap();
		let block1 = H256::from([12u8; 32]);
		let block2 = H256::from([13u8; 32]);
		let prefix = b"system:";

		// Set progress on block1 only
		cache.update_prefix_scan(block1, prefix, b"system:key1", true).await.unwrap();

		// Block1 has progress
		let p1 = cache.get_prefix_scan_progress(block1, prefix).await.unwrap();
		assert!(p1.is_some());
		assert!(p1.unwrap().is_complete);

		// Block2 has no progress
		let p2 = cache.get_prefix_scan_progress(block2, prefix).await.unwrap();
		assert!(p2.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_keys_by_prefix_works() {
		let cache = StorageCache::in_memory().await.unwrap();
		let block_hash = H256::from([14u8; 32]);

		// Insert keys with different prefixes
		let entries: Vec<(&[u8], Option<&[u8]>)> = vec![
			(b"tokens:alice", Some(b"100")),
			(b"tokens:bob", Some(b"200")),
			(b"tokens:charlie", Some(b"300")),
			(b"balances:alice", Some(b"50")),
			(b"balances:bob", Some(b"75")),
		];
		cache.set_storage_batch(block_hash, &entries).await.unwrap();

		// Get keys with "tokens:" prefix
		let token_keys = cache.get_keys_by_prefix(block_hash, b"tokens:").await.unwrap();
		assert_eq!(token_keys.len(), 3);
		assert!(token_keys.contains(&b"tokens:alice".to_vec()));
		assert!(token_keys.contains(&b"tokens:bob".to_vec()));
		assert!(token_keys.contains(&b"tokens:charlie".to_vec()));

		// Get keys with "balances:" prefix
		let balance_keys = cache.get_keys_by_prefix(block_hash, b"balances:").await.unwrap();
		assert_eq!(balance_keys.len(), 2);
		assert!(balance_keys.contains(&b"balances:alice".to_vec()));
		assert!(balance_keys.contains(&b"balances:bob".to_vec()));

		// Get keys with non-existent prefix
		let empty_keys = cache.get_keys_by_prefix(block_hash, b"nonexistent:").await.unwrap();
		assert!(empty_keys.is_empty());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn count_keys_by_prefix_works() {
		let cache = StorageCache::in_memory().await.unwrap();
		let block_hash = H256::from([15u8; 32]);

		// Insert keys with different prefixes
		let entries: Vec<(&[u8], Option<&[u8]>)> = vec![
			(b"prefix_a:1", Some(b"v1")),
			(b"prefix_a:2", Some(b"v2")),
			(b"prefix_a:3", Some(b"v3")),
			(b"prefix_b:1", Some(b"v4")),
		];
		cache.set_storage_batch(block_hash, &entries).await.unwrap();

		assert_eq!(cache.count_keys_by_prefix(block_hash, b"prefix_a:").await.unwrap(), 3);
		assert_eq!(cache.count_keys_by_prefix(block_hash, b"prefix_b:").await.unwrap(), 1);
		assert_eq!(cache.count_keys_by_prefix(block_hash, b"prefix_c:").await.unwrap(), 0);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn next_key_from_cache_works() {
		let cache = StorageCache::in_memory().await.unwrap();
		let block_hash = H256::from([20u8; 32]);

		// Insert keys with a prefix
		let entries: Vec<(&[u8], Option<&[u8]>)> = vec![
			(b"prefix:aaa", Some(b"v1")),
			(b"prefix:bbb", Some(b"v2")),
			(b"prefix:ccc", Some(b"v3")),
			(b"other:ddd", Some(b"v4")),
		];
		cache.set_storage_batch(block_hash, &entries).await.unwrap();

		// Next key after "prefix:aaa" with prefix "prefix:" should be "prefix:bbb"
		let next = cache.next_key_from_cache(block_hash, b"prefix:", b"prefix:aaa").await.unwrap();
		assert_eq!(next, Some(b"prefix:bbb".to_vec()));

		// Next key after "prefix:bbb" should be "prefix:ccc"
		let next = cache.next_key_from_cache(block_hash, b"prefix:", b"prefix:bbb").await.unwrap();
		assert_eq!(next, Some(b"prefix:ccc".to_vec()));

		// Next key after "prefix:ccc" should be None (no more keys)
		let next = cache.next_key_from_cache(block_hash, b"prefix:", b"prefix:ccc").await.unwrap();
		assert!(next.is_none());

		// Next key from the very start with prefix "prefix:" should be "prefix:aaa"
		let next = cache.next_key_from_cache(block_hash, b"prefix:", b"prefix:").await.unwrap();
		assert_eq!(next, Some(b"prefix:aaa".to_vec()));
	}

	#[test]
	fn increment_prefix_works() {
		// Normal case
		assert_eq!(increment_prefix(b"abc"), Some(b"abd".to_vec()));

		// Increment last byte
		assert_eq!(increment_prefix(b"ab\xff"), Some(b"ac".to_vec()));

		// Multiple 0xff bytes
		assert_eq!(increment_prefix(b"a\xff\xff"), Some(b"b".to_vec()));

		// All 0xff - no valid increment possible
		assert_eq!(increment_prefix(b"\xff\xff\xff"), None);

		// Empty prefix - no increment possible
		assert_eq!(increment_prefix(b""), None);

		// Single byte
		assert_eq!(increment_prefix(b"a"), Some(b"b".to_vec()));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn clear_block_removes_prefix_scans() {
		let cache = StorageCache::in_memory().await.unwrap();
		let hash = H256::from([16u8; 32]);
		let prefix = b"test:";

		// Set up prefix scan progress
		cache.update_prefix_scan(hash, prefix, b"test:key", true).await.unwrap();
		assert!(cache.get_prefix_scan_progress(hash, prefix).await.unwrap().is_some());

		// Clear block
		cache.clear_block(hash).await.unwrap();

		// Prefix scan progress should be removed
		assert!(cache.get_prefix_scan_progress(hash, prefix).await.unwrap().is_none());
	}

	// Tests for local storage with validity

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_key_returns_none_for_nonexistent_key() {
		let cache = StorageCache::in_memory().await.unwrap();

		let result = cache.get_local_key(b"nonexistent_key").await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn insert_local_key_creates_new_key() {
		let cache = StorageCache::in_memory().await.unwrap();
		let key = b"new_key";

		// Insert new key
		let key_id = cache.insert_local_key(key).await.unwrap();
		assert_eq!(key_id, 1);

		// Verify it exists
		let result = cache.get_local_key(key).await.unwrap();
		assert!(result.is_some());
		assert_eq!(result.unwrap().id, key_id);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn insert_local_key_returns_existing_id() {
		let cache = StorageCache::in_memory().await.unwrap();
		let key = b"duplicate_key";

		// Insert key twice
		let key_id1 = cache.insert_local_key(key).await.unwrap();
		let key_id2 = cache.insert_local_key(key).await.unwrap();

		// Should return the same ID
		assert_eq!(key_id1, key_id2);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn insert_and_get_local_value_at_block() {
		let cache = StorageCache::in_memory().await.unwrap();
		let key = b"test_key";
		let value = b"test_value";

		// Insert key and value
		let key_id = cache.insert_local_key(key).await.unwrap();
		cache.insert_local_value(key_id, Some(value), 100).await.unwrap();

		// Query at block 100 - should find it (valid_from = 100, valid_until = NULL)
		let result = cache.get_local_value_at_block(key, 100).await.unwrap();
		assert_eq!(result, Some(Some(value.to_vec())));

		// Query at block 150 - should still find it (valid_until = NULL means still valid)
		let result = cache.get_local_value_at_block(key, 150).await.unwrap();
		assert_eq!(result, Some(Some(value.to_vec())));

		// Query at block 99 - should not find it (before valid_from)
		let result = cache.get_local_value_at_block(key, 99).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_value_at_block_nonexistent_key() {
		let cache = StorageCache::in_memory().await.unwrap();

		let result = cache.get_local_value_at_block(b"nonexistent", 100).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn close_local_value_sets_valid_until() {
		let cache = StorageCache::in_memory().await.unwrap();
		let key = b"closing_key";
		let value1 = b"value1";
		let value2 = b"value2";

		// Insert key and first value at block 100
		let key_id = cache.insert_local_key(key).await.unwrap();
		cache.insert_local_value(key_id, Some(value1), 100).await.unwrap();

		// Close at block 150 and insert new value
		cache.close_local_value(key_id, 150).await.unwrap();
		cache.insert_local_value(key_id, Some(value2), 150).await.unwrap();

		// Query at block 120 - should get value1
		let result = cache.get_local_value_at_block(key, 120).await.unwrap();
		assert_eq!(result, Some(Some(value1.to_vec())));

		// Query at block 150 - should get value2
		let result = cache.get_local_value_at_block(key, 150).await.unwrap();
		assert_eq!(result, Some(Some(value2.to_vec())));

		// Query at block 200 - should get value2 (still valid)
		let result = cache.get_local_value_at_block(key, 200).await.unwrap();
		assert_eq!(result, Some(Some(value2.to_vec())));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_values_at_block_batch_works() {
		let cache = StorageCache::in_memory().await.unwrap();

		let key1 = b"batch_key1";
		let key2 = b"batch_key2";
		let key3 = b"batch_key3";
		let value1 = b"batch_value1";
		let value2 = b"batch_value2";

		// Insert keys and values
		let key_id1 = cache.insert_local_key(key1).await.unwrap();
		let key_id2 = cache.insert_local_key(key2).await.unwrap();
		cache.insert_local_value(key_id1, Some(value1), 100).await.unwrap();
		cache.insert_local_value(key_id2, Some(value2), 100).await.unwrap();

		// Batch query
		let keys: Vec<&[u8]> = vec![key1, key2, key3];
		let results = cache.get_local_values_at_block_batch(&keys, 100).await.unwrap();

		assert_eq!(results.len(), 3);
		assert_eq!(results[0], Some(Some(value1.to_vec())));
		assert_eq!(results[1], Some(Some(value2.to_vec())));
		assert!(results[2].is_none()); // key3 doesn't exist
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_values_at_block_batch_respects_validity() {
		let cache = StorageCache::in_memory().await.unwrap();

		let key = b"validity_key";
		let value1 = b"value_v1";
		let value2 = b"value_v2";

		// Insert key and values with validity ranges
		let key_id = cache.insert_local_key(key).await.unwrap();
		cache.insert_local_value(key_id, Some(value1), 100).await.unwrap();
		cache.close_local_value(key_id, 200).await.unwrap();
		cache.insert_local_value(key_id, Some(value2), 200).await.unwrap();

		// Query at different blocks
		let keys: Vec<&[u8]> = vec![key];

		let results = cache.get_local_values_at_block_batch(&keys, 150).await.unwrap();
		assert_eq!(results[0], Some(Some(value1.to_vec())));

		let results = cache.get_local_values_at_block_batch(&keys, 200).await.unwrap();
		assert_eq!(results[0], Some(Some(value2.to_vec())));

		let results = cache.get_local_values_at_block_batch(&keys, 99).await.unwrap();
		assert!(results[0].is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_values_at_block_batch_with_duplicate_keys() {
		let cache = StorageCache::in_memory().await.unwrap();

		let key = b"dup_key";
		let keys: Vec<&[u8]> = vec![key, key]; // duplicate

		let result = cache.get_local_values_at_block_batch(&keys, 100).await;
		assert!(matches!(result, Err(CacheError::DuplicatedKeys)));
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn clear_local_storage_removes_all_data() {
		let cache = StorageCache::in_memory().await.unwrap();

		let key1 = b"clear_key1";
		let key2 = b"clear_key2";
		let value = b"some_value";

		// Insert some data
		let key_id1 = cache.insert_local_key(key1).await.unwrap();
		let key_id2 = cache.insert_local_key(key2).await.unwrap();
		cache.insert_local_value(key_id1, Some(value), 100).await.unwrap();
		cache.insert_local_value(key_id2, Some(value), 100).await.unwrap();

		// Verify data exists
		assert!(cache.get_local_key(key1).await.unwrap().is_some());
		assert!(cache.get_local_key(key2).await.unwrap().is_some());
		assert!(cache.get_local_value_at_block(key1, 100).await.unwrap().is_some());

		// Clear all local storage
		cache.clear_local_storage().await.unwrap();

		// Verify data is gone
		assert!(cache.get_local_key(key1).await.unwrap().is_none());
		assert!(cache.get_local_key(key2).await.unwrap().is_none());
		assert!(cache.get_local_value_at_block(key1, 100).await.unwrap().is_none());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_keys_at_block_returns_live_keys() {
		let cache = StorageCache::in_memory().await.unwrap();

		// Insert keys with prefix "pallet:" at different blocks.
		let k1 = b"pallet:alice";
		let k2 = b"pallet:bob";
		let k3 = b"other:charlie";

		let id1 = cache.insert_local_key(k1).await.unwrap();
		let id2 = cache.insert_local_key(k2).await.unwrap();
		let id3 = cache.insert_local_key(k3).await.unwrap();

		// k1: valid from block 100
		cache.insert_local_value(id1, Some(b"v1"), 100).await.unwrap();
		// k2: valid from block 200
		cache.insert_local_value(id2, Some(b"v2"), 200).await.unwrap();
		// k3: valid from block 100 (different prefix)
		cache.insert_local_value(id3, Some(b"v3"), 100).await.unwrap();

		// At block 150: only k1 matches "pallet:" prefix
		let keys = cache.get_local_keys_at_block(b"pallet:", 150).await.unwrap();
		assert_eq!(keys, vec![k1.to_vec()]);

		// At block 200: both k1 and k2 match
		let keys = cache.get_local_keys_at_block(b"pallet:", 200).await.unwrap();
		assert_eq!(keys, vec![k1.to_vec(), k2.to_vec()]);

		// At block 99: nothing matches (before any inserts)
		let keys = cache.get_local_keys_at_block(b"pallet:", 99).await.unwrap();
		assert!(keys.is_empty());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_keys_at_block_excludes_deleted() {
		let cache = StorageCache::in_memory().await.unwrap();

		let k1 = b"pallet:alice";
		let id1 = cache.insert_local_key(k1).await.unwrap();

		// k1: value at block 100, deleted at block 200
		cache.insert_local_value(id1, Some(b"v1"), 100).await.unwrap();
		cache.close_local_value(id1, 200).await.unwrap();
		cache.insert_local_value(id1, None, 200).await.unwrap();

		// At block 150: key exists (has value)
		let keys = cache.get_local_keys_at_block(b"pallet:", 150).await.unwrap();
		assert_eq!(keys, vec![k1.to_vec()]);

		// At block 200: key was deleted (value is NULL)
		let keys = cache.get_local_keys_at_block(b"pallet:", 200).await.unwrap();
		assert!(keys.is_empty());
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn get_local_deleted_keys_at_block_works() {
		let cache = StorageCache::in_memory().await.unwrap();

		let k1 = b"pallet:alice";
		let id1 = cache.insert_local_key(k1).await.unwrap();

		// k1: value at block 100, deleted at block 200
		cache.insert_local_value(id1, Some(b"v1"), 100).await.unwrap();
		cache.close_local_value(id1, 200).await.unwrap();
		cache.insert_local_value(id1, None, 200).await.unwrap();

		// At block 150: no deleted keys
		let deleted = cache.get_local_deleted_keys_at_block(b"pallet:", 150).await.unwrap();
		assert!(deleted.is_empty());

		// At block 200: k1 is deleted
		let deleted = cache.get_local_deleted_keys_at_block(b"pallet:", 200).await.unwrap();
		assert_eq!(deleted, vec![k1.to_vec()]);
	}

	#[tokio::test(flavor = "multi_thread")]
	async fn prefix_upper_bound_works() {
		// Normal case
		assert_eq!(StorageCache::prefix_upper_bound(b"abc"), Some(b"abd".to_vec()));
		// Trailing 0xFF
		assert_eq!(StorageCache::prefix_upper_bound(b"ab\xff"), Some(b"ac".to_vec()));
		// All 0xFF
		assert_eq!(StorageCache::prefix_upper_bound(b"\xff\xff"), None);
		// Empty prefix
		assert_eq!(StorageCache::prefix_upper_bound(b""), None);
	}
}
