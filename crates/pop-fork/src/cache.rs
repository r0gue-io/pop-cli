// SPDX-License-Identifier: GPL-3.0

//! SQLite-based storage cache for fork operations.
//!
//! Provides persistent caching of storage values fetched from live chains,
//! enabling fast restarts and reducing RPC calls.

use crate::{
	error::cache::CacheError,
	models::{BlockRow, NewBlockRow, NewPrefixScanRow, NewStorageRow},
	schema::{blocks, prefix_scans, storage},
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
enum ConnectionGuard<'a> {
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
	async fn get_conn(&self) -> Result<ConnectionGuard<'_>, CacheError> {
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

	/// Clear all cached data for a specific block.
	pub async fn clear_block(&self, hash: H256) -> Result<(), CacheError> {
		// Use a transaction to ensure both deletes succeed or fail together.
		// This maintains consistency: we never have orphaned storage entries
		// without their parent block, or vice versa.
		use crate::schema::{
			blocks::columns as bc, prefix_scans::columns as psc, storage::columns as sc,
		};
		let orig_bh_vec = hash.as_bytes().to_vec();
		use crate::schema::{blocks::columns as bc, storage::columns as sc};
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
		let bh = block_hash.as_bytes().to_vec();
		let p = prefix.to_vec();

		let row: Option<(Option<Vec<u8>>, bool)> = match &self.inner {
			StorageConn::Pool(pool) => {
				let mut conn = pool.get().await?;
				prefix_scans::table
					.filter(psc::block_hash.eq(bh))
					.filter(psc::prefix.eq(p))
					.select((psc::last_scanned_key, psc::is_complete))
					.first::<(Option<Vec<u8>>, bool)>(&mut conn)
					.await
					.optional()?
			},
			StorageConn::Single(m) => {
				let mut conn = m.lock().await;
				prefix_scans::table
					.filter(psc::block_hash.eq(bh))
					.filter(psc::prefix.eq(p))
					.select((psc::last_scanned_key, psc::is_complete))
					.first::<(Option<Vec<u8>>, bool)>(&mut *conn)
					.await
					.optional()?
			},
		};

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
			block_hash: block_hash.as_bytes().to_vec(),
			prefix: prefix.to_vec(),
			last_scanned_key: Some(last_key.to_vec()),
			is_complete,
		};

		let mut attempts = 0;
		loop {
			// Use excluded() to reference inserted values in DO UPDATE,
			// avoiding redundant clones on each retry
			let res = match &self.inner {
				StorageConn::Pool(pool) => {
					let mut conn = pool.get().await?;
					diesel::insert_into(prefix_scans::table)
						.values(&new_row)
						.on_conflict((psc::block_hash, psc::prefix))
						.do_update()
						.set((
							psc::last_scanned_key.eq(excluded(psc::last_scanned_key)),
							psc::is_complete.eq(excluded(psc::is_complete)),
						))
						.execute(&mut conn)
						.await
				},
				StorageConn::Single(m) => {
					let mut conn = m.lock().await;
					diesel::insert_into(prefix_scans::table)
						.values(&new_row)
						.on_conflict((psc::block_hash, psc::prefix))
						.do_update()
						.set((
							psc::last_scanned_key.eq(excluded(psc::last_scanned_key)),
							psc::is_complete.eq(excluded(psc::is_complete)),
						))
						.execute(&mut *conn)
						.await
				},
			};
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
		let bh = block_hash.as_bytes().to_vec();

		// SQLite BLOB comparison with >= and < for prefix range
		let prefix_end = increment_prefix(prefix);

		let keys: Vec<Vec<u8>> = match &self.inner {
			StorageConn::Pool(pool) => {
				let mut conn = pool.get().await?;
				let mut query = storage::table
					.filter(sc::block_hash.eq(&bh))
					.filter(sc::key.ge(prefix))
					.select(sc::key)
					.into_boxed();

				if let Some(ref end) = prefix_end {
					query = query.filter(sc::key.lt(end));
				}

				query.load::<Vec<u8>>(&mut conn).await?
			},
			StorageConn::Single(m) => {
				let mut conn = m.lock().await;
				let mut query = storage::table
					.filter(sc::block_hash.eq(&bh))
					.filter(sc::key.ge(prefix))
					.select(sc::key)
					.into_boxed();

				if let Some(ref end) = prefix_end {
					query = query.filter(sc::key.lt(end));
				}

				query.load::<Vec<u8>>(&mut *conn).await?
			},
		};

		Ok(keys)
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
		let bh = block_hash.as_bytes().to_vec();
		let prefix_end = increment_prefix(prefix);

		let count: i64 = match &self.inner {
			StorageConn::Pool(pool) => {
				let mut conn = pool.get().await?;
				let mut query = storage::table
					.filter(sc::block_hash.eq(&bh))
					.filter(sc::key.ge(prefix))
					.into_boxed();

				if let Some(ref end) = prefix_end {
					query = query.filter(sc::key.lt(end));
				}

				query.count().get_result(&mut conn).await?
			},
			StorageConn::Single(m) => {
				let mut conn = m.lock().await;
				let mut query = storage::table
					.filter(sc::block_hash.eq(&bh))
					.filter(sc::key.ge(prefix))
					.into_boxed();

				if let Some(ref end) = prefix_end {
					query = query.filter(sc::key.lt(end));
				}

				query.count().get_result(&mut *conn).await?
			},
		};

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
		cache.set_batch(block_hash, &entries).await.unwrap();

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
		cache.set_batch(block_hash, &entries).await.unwrap();

		assert_eq!(cache.count_keys_by_prefix(block_hash, b"prefix_a:").await.unwrap(), 3);
		assert_eq!(cache.count_keys_by_prefix(block_hash, b"prefix_b:").await.unwrap(), 1);
		assert_eq!(cache.count_keys_by_prefix(block_hash, b"prefix_c:").await.unwrap(), 0);
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
}
