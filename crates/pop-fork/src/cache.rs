// SPDX-License-Identifier: GPL-3.0

//! SQLite-based storage cache for fork operations.
//!
//! Provides persistent caching of storage values fetched from live chains,
//! enabling fast restarts and reducing RPC calls.

use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};
use std::path::Path;
use subxt::config::substrate::H256;
use thiserror::Error;

/// Maximum number of connections in the SQLite connection pool.
///
/// Since Pop is the only process accessing the database, this is for internal
/// async task concurrency. 5 provides comfortable headroom for parallel operations
/// while remaining lightweight on end-user devices.
const MAX_POOL_CONNECTIONS: u32 = 5;

/// SQLite connection string for in-memory databases.
#[cfg(test)]
const SQLITE_MEMORY_URL: &str = "sqlite::memory:";

/// Connection pool size for in-memory databases.
///
/// Must be 1 because SQLite in-memory databases are connection-specific:
/// each connection creates a separate, isolated database instance.
#[cfg(test)]
const MEMORY_POOL_CONNECTIONS: u32 = 1;

/// Errors that can occur when interacting with the storage cache.
#[derive(Debug, Error)]
pub enum CacheError {
	/// Database error.
	#[error("Database error: {0}")]
	Database(#[from] sqlx::Error),
	/// IO error.
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),
	/// Data corruption detected in the cache.
	#[error("Data corruption: {0}")]
	DataCorruption(String),
}

/// Information about a cached block.
///
/// # Block Number Type
///
/// Block numbers are stored as `u32` to match Polkadot SDK's `BlockNumber` type.
/// SQLite stores all integers as `i64`, so we convert when reading from the database.
/// Invalid values (negative or > u32::MAX) indicate database corruption and will
/// return a [`CacheError::DataCorruption`] error.
#[derive(Debug, Clone)]
pub struct BlockInfo {
	/// Block hash.
	pub hash: H256,
	/// Block number.
	pub number: u32,
	/// SCALE-encoded block header.
	pub header: Vec<u8>,
	/// Parent block hash.
	pub parent_hash: H256,
}

/// SQLite-backed persistent cache for storage values.
///
/// Enables fast restarts without re-fetching all data from live chains
/// and reduces load on public RPC endpoints.
pub struct StorageCache {
	pool: SqlitePool,
}

impl StorageCache {
	/// Open or create a cache database at the specified path.
	///
	/// Creates the parent directory if it doesn't exist.
	pub async fn open(path: &Path) -> Result<Self, CacheError> {
		// Ensure parent directory exists
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent)?;
		}

		let url = format!("sqlite:{}?mode=rwc", path.display());
		let pool = SqlitePoolOptions::new()
			.max_connections(MAX_POOL_CONNECTIONS)
			.connect(&url)
			.await?;

		// Create tables
		sqlx::query(CREATE_TABLES_SQL).execute(&pool).await?;

		Ok(Self { pool })
	}

	/// Open an in-memory cache (for testing).
	#[cfg(test)]
	pub async fn in_memory() -> Result<Self, CacheError> {
		let pool = SqlitePoolOptions::new()
			.max_connections(MEMORY_POOL_CONNECTIONS)
			.connect(SQLITE_MEMORY_URL)
			.await?;

		sqlx::query(CREATE_TABLES_SQL).execute(&pool).await?;

		Ok(Self { pool })
	}

	/// Get a cached storage value.
	///
	/// # Returns
	/// * `Ok(Some(Some(value)))` - Cached with a value
	/// * `Ok(Some(None))` - Cached as empty (storage key exists but has no value)
	/// * `Ok(None)` - Not in cache (unknown)
	pub async fn get(
		&self,
		block_hash: H256,
		key: &[u8],
	) -> Result<Option<Option<Vec<u8>>>, CacheError> {
		// Retrieve the cached value and its empty flag for the given block and key.
		// We need both `value` and `is_empty` to distinguish between:
		// - Key not in cache (no row returned)
		// - Key cached as empty (row exists, is_empty = true)
		// - Key cached with value (row exists, is_empty = false)
		let row =
			sqlx::query("SELECT value, is_empty FROM storage WHERE block_hash = ? AND key = ?")
				.bind(block_hash.as_bytes())
				.bind(key)
				.fetch_optional(&self.pool)
				.await?;

		Ok(row.map(|r| {
			let is_empty: bool = r.get("is_empty");
			if is_empty { None } else { Some(r.get("value")) }
		}))
	}

	/// Cache a storage value.
	///
	/// # Arguments
	/// * `block_hash` - The block hash this storage is from
	/// * `key` - The storage key
	/// * `value` - The storage value, or None if the key has no value (empty)
	pub async fn set(
		&self,
		block_hash: H256,
		key: &[u8],
		value: Option<&[u8]>,
	) -> Result<(), CacheError> {
		// Insert or update the cached storage entry.
		// Uses INSERT OR REPLACE (SQLite's UPSERT) to handle both new entries
		// and updates to existing entries with the same (block_hash, key) primary key.
		// The `is_empty` flag is set based on whether value is None, allowing us
		// to cache the knowledge that a storage key has no value (vs not being cached).
		sqlx::query(
			"INSERT OR REPLACE INTO storage (block_hash, key, value, is_empty) VALUES (?, ?, ?, ?)",
		)
		.bind(block_hash.as_bytes())
		.bind(key)
		.bind(value)
		.bind(value.is_none())
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	/// Get multiple cached storage values in a batch.
	///
	/// Returns results in the same order as the input keys.
	pub async fn get_batch(
		&self,
		block_hash: H256,
		keys: &[&[u8]],
	) -> Result<Vec<Option<Option<Vec<u8>>>>, CacheError> {
		if keys.is_empty() {
			return Ok(vec![]);
		}

		// Build a SELECT query with dynamic IN clause for batch retrieval.
		// This fetches all requested keys in a single round-trip to the database,
		// which is more efficient than individual queries when fetching many keys.
		// Example: SELECT ... WHERE block_hash = ? AND key IN (?, ?, ?)
		let placeholders: Vec<_> = keys.iter().map(|_| "?").collect();
		let query = format!(
			"SELECT key, value, is_empty FROM storage WHERE block_hash = ? AND key IN ({})",
			placeholders.join(", ")
		);

		let mut query_builder = sqlx::query(&query).bind(block_hash.as_bytes());

		for key in keys {
			query_builder = query_builder.bind(key);
		}

		let rows = query_builder.fetch_all(&self.pool).await?;

		// Build a map from the results. SQLite doesn't guarantee result order matches
		// the IN clause order, so we use a HashMap to look up values by key.
		let mut cache_map: std::collections::HashMap<Vec<u8>, Option<Vec<u8>>> =
			std::collections::HashMap::new();
		for row in rows {
			let key: Vec<u8> = row.get("key");
			let is_empty: bool = row.get("is_empty");
			let value = if is_empty { None } else { Some(row.get("value")) };
			cache_map.insert(key, value);
		}

		// Return values in the same order as input keys.
		// Keys not found in cache_map (not in DB) return None (not cached).
		// Keys found return Some(value) where value is None for empty or Some(bytes) for data.
		Ok(keys.iter().map(|key| cache_map.get(*key).cloned()).collect())
	}

	/// Cache multiple storage values in a batch.
	///
	/// Uses a transaction for efficiency.
	pub async fn set_batch(
		&self,
		block_hash: H256,
		entries: &[(&[u8], Option<&[u8]>)],
	) -> Result<(), CacheError> {
		if entries.is_empty() {
			return Ok(());
		}

		// Use a transaction to batch all inserts together.
		// This is significantly faster than individual inserts because:
		// 1. SQLite commits are expensive (fsync to disk)
		// 2. A transaction groups all inserts into a single commit
		// 3. If any insert fails, the entire batch is rolled back
		let mut tx = self.pool.begin().await?;

		for (key, value) in entries {
			// Same INSERT OR REPLACE logic as set(), executed within the transaction.
			sqlx::query(
				"INSERT OR REPLACE INTO storage (block_hash, key, value, is_empty) VALUES (?, ?, ?, ?)",
			)
			.bind(block_hash.as_bytes())
			.bind(key)
			.bind(value.as_deref())
			.bind(value.is_none())
			.execute(&mut *tx)
			.await?;
		}

		tx.commit().await?;
		Ok(())
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
		// Uses INSERT OR REPLACE to update if the block was previously cached
		// (e.g., if header data was incomplete and needs updating).
		sqlx::query(
			"INSERT OR REPLACE INTO blocks (hash, number, parent_hash, header) VALUES (?, ?, ?, ?)",
		)
		.bind(hash.as_bytes())
		.bind(number)
		.bind(parent_hash.as_bytes())
		.bind(header)
		.execute(&self.pool)
		.await?;

		Ok(())
	}

	/// Get cached block metadata.
	pub async fn get_block(&self, hash: H256) -> Result<Option<BlockInfo>, CacheError> {
		// Retrieve all block metadata fields by the block's hash (primary key).
		// Returns None if the block hasn't been cached yet.
		let row =
			sqlx::query("SELECT hash, number, parent_hash, header FROM blocks WHERE hash = ?")
				.bind(hash.as_bytes())
				.fetch_optional(&self.pool)
				.await?;

		// Convert SQLite BLOB and INTEGER types back to their Rust equivalents.
		// Note: SQLite stores integers as i64, so we safely convert to u32 for block numbers.
		let Some(r) = row else {
			return Ok(None);
		};

		let hash_bytes: Vec<u8> = r.get("hash");
		let parent_bytes: Vec<u8> = r.get("parent_hash");
		let number: u32 = r
			.get::<i64, _>("number")
			.try_into()
			.map_err(|_| CacheError::DataCorruption("block number out of u32 range".into()))?;

		Ok(Some(BlockInfo {
			hash: H256::from_slice(&hash_bytes),
			number,
			parent_hash: H256::from_slice(&parent_bytes),
			header: r.get("header"),
		}))
	}

	/// Clear all cached data for a specific block.
	pub async fn clear_block(&self, hash: H256) -> Result<(), CacheError> {
		// Use a transaction to ensure both deletes succeed or fail together.
		// This maintains consistency: we never have orphaned storage entries
		// without their parent block, or vice versa.
		let mut tx = self.pool.begin().await?;

		// Delete all storage entries associated with this block.
		// The idx_storage_block index makes this lookup efficient.
		sqlx::query("DELETE FROM storage WHERE block_hash = ?")
			.bind(hash.as_bytes())
			.execute(&mut *tx)
			.await?;

		// Delete the block metadata itself.
		sqlx::query("DELETE FROM blocks WHERE hash = ?")
			.bind(hash.as_bytes())
			.execute(&mut *tx)
			.await?;

		tx.commit().await?;
		Ok(())
	}
}

/// SQL to create the cache tables.
///
/// Schema design:
/// - `storage`: Caches individual storage key-value pairs per block.
///   - Composite primary key (block_hash, key) ensures uniqueness per block.
///   - `is_empty` flag distinguishes "cached as empty" from "not cached".
///   - Index on block_hash speeds up clearing all storage for a block.
///
/// - `blocks`: Caches block metadata (header, parent hash, number).
///   - Primary key on hash for O(1) lookups by block hash.
///   - Index on number supports lookups by block number.
///
/// Both tables use BLOB for hashes/keys since they're arbitrary byte sequences.
/// Uses IF NOT EXISTS for idempotent initialization (safe to call multiple times).
const CREATE_TABLES_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS storage (
    block_hash BLOB NOT NULL,
    key BLOB NOT NULL,
    value BLOB,
    is_empty BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (block_hash, key)
);

CREATE INDEX IF NOT EXISTS idx_storage_block ON storage(block_hash);

CREATE TABLE IF NOT EXISTS blocks (
    hash BLOB PRIMARY KEY,
    number INTEGER NOT NULL,
    parent_hash BLOB NOT NULL,
    header BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_blocks_number ON blocks(number);
"#;

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn in_memory_cache_works() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block_hash = H256::from([1u8; 32]);
		let key = b"test_key";
		let value = b"test_value";

		// Initially not cached
		assert!(cache.get(block_hash, key).await.unwrap().is_none());

		// Set a value
		cache.set(block_hash, key, Some(value)).await.unwrap();

		// Now cached with value
		let cached = cache.get(block_hash, key).await.unwrap();
		assert_eq!(cached, Some(Some(value.to_vec())));
	}

	#[tokio::test]
	async fn cache_empty_value() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block_hash = H256::from([2u8; 32]);
		let key = b"empty_key";

		// Set as empty (key exists but no value)
		cache.set(block_hash, key, None).await.unwrap();

		// Cached as empty
		let cached = cache.get(block_hash, key).await.unwrap();
		assert_eq!(cached, Some(None));
	}

	#[tokio::test]
	async fn batch_operations() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block_hash = H256::from([3u8; 32]);
		let entries: Vec<(&[u8], Option<&[u8]>)> = vec![
			(b"key1", Some(b"value1")),
			(b"key2", Some(b"value2")),
			(b"key3", None), // empty
		];

		// Batch set
		cache.set_batch(block_hash, &entries).await.unwrap();

		// Batch get
		let keys: Vec<&[u8]> = vec![b"key1", b"key2", b"key3", b"key4"];
		let results = cache.get_batch(block_hash, &keys).await.unwrap();

		assert_eq!(results.len(), 4);
		assert_eq!(results[0], Some(Some(b"value1".to_vec())));
		assert_eq!(results[1], Some(Some(b"value2".to_vec())));
		assert_eq!(results[2], Some(None)); // empty
		assert_eq!(results[3], None); // not cached
	}

	#[tokio::test]
	async fn block_caching() {
		let cache = StorageCache::in_memory().await.unwrap();

		let hash = H256::from([4u8; 32]);
		let parent_hash = H256::from([3u8; 32]);
		let header = b"mock_header_data";

		// Cache block
		cache.cache_block(hash, 100, parent_hash, header).await.unwrap();

		// Get block
		let block = cache.get_block(hash).await.unwrap().unwrap();
		assert_eq!(block.hash, hash);
		assert_eq!(block.number, 100);
		assert_eq!(block.parent_hash, parent_hash);
		assert_eq!(block.header, header.to_vec());
	}

	#[tokio::test]
	async fn different_blocks_have_separate_storage() {
		let cache = StorageCache::in_memory().await.unwrap();

		let block1 = H256::from([5u8; 32]);
		let block2 = H256::from([6u8; 32]);
		let key = b"same_key";

		cache.set(block1, key, Some(b"value1")).await.unwrap();
		cache.set(block2, key, Some(b"value2")).await.unwrap();

		let cached1 = cache.get(block1, key).await.unwrap();
		let cached2 = cache.get(block2, key).await.unwrap();

		assert_eq!(cached1, Some(Some(b"value1".to_vec())));
		assert_eq!(cached2, Some(Some(b"value2".to_vec())));
	}

	#[tokio::test]
	async fn clear_block_removes_data() {
		let cache = StorageCache::in_memory().await.unwrap();

		let hash = H256::from([7u8; 32]);
		let parent_hash = H256::from([6u8; 32]);
		let key = b"test_key";

		cache.set(hash, key, Some(b"value")).await.unwrap();
		cache.cache_block(hash, 50, parent_hash, b"header").await.unwrap();

		// Data exists
		assert!(cache.get(hash, key).await.unwrap().is_some());
		assert!(cache.get_block(hash).await.unwrap().is_some());

		// Clear
		cache.clear_block(hash).await.unwrap();

		// Data removed
		assert!(cache.get(hash, key).await.unwrap().is_none());
		assert!(cache.get_block(hash).await.unwrap().is_none());
	}

	#[tokio::test]
	async fn file_persistence() {
		let temp_dir = tempfile::tempdir().unwrap();
		let db_path = temp_dir.path().join("test_cache.db");

		let block_hash = H256::from([8u8; 32]);
		let key = b"persistent_key";
		let value = b"persistent_value";

		// Write and close
		{
			let cache = StorageCache::open(&db_path).await.unwrap();
			cache.set(block_hash, key, Some(value)).await.unwrap();
		}

		// Reopen and verify
		{
			let cache = StorageCache::open(&db_path).await.unwrap();
			let cached = cache.get(block_hash, key).await.unwrap();
			assert_eq!(cached, Some(Some(value.to_vec())));
		}
	}

	#[tokio::test]
	async fn concurrent_access() {
		use std::sync::Arc;

		let temp_dir = tempfile::tempdir().unwrap();
		let db_path = temp_dir.path().join("concurrent_test.db");
		let cache = Arc::new(StorageCache::open(&db_path).await.unwrap());

		let block_hash = H256::from([9u8; 32]);

		// Spawn multiple concurrent write tasks
		let mut handles = vec![];
		for i in 0..10u8 {
			let cache = Arc::clone(&cache);
			let handle = tokio::spawn(async move {
				let key = format!("key_{}", i);
				let value = format!("value_{}", i);
				cache.set(block_hash, key.as_bytes(), Some(value.as_bytes())).await
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
			let cache = Arc::clone(&cache);
			let handle = tokio::spawn(async move {
				let key = format!("key_{}", i);
				cache.get(block_hash, key.as_bytes()).await
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
		let cache1 = Arc::clone(&cache);
		let cache2 = Arc::clone(&cache);
		let block_hash2 = H256::from([10u8; 32]);

		let batch_handle1 = tokio::spawn(async move {
			let keys: Vec<Vec<u8>> = (0..5).map(|i| format!("batch1_{}", i).into_bytes()).collect();
			let values: Vec<Vec<u8>> = (0..5).map(|i| vec![i]).collect();
			let entries: Vec<(&[u8], Option<&[u8]>)> = keys
				.iter()
				.zip(values.iter())
				.map(|(k, v)| (k.as_slice(), Some(v.as_slice())))
				.collect();
			cache1.set_batch(block_hash2, &entries).await
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
			cache2.set_batch(block_hash2, &entries).await
		});

		batch_handle1.await.unwrap().unwrap();
		batch_handle2.await.unwrap().unwrap();

		// Verify batch results
		let keys: Vec<Vec<u8>> = (0..5).map(|i| format!("batch1_{}", i).into_bytes()).collect();
		let key_refs: Vec<&[u8]> = keys.iter().map(|k| k.as_slice()).collect();
		let results = cache.get_batch(block_hash2, &key_refs).await.unwrap();
		for (i, result) in results.iter().enumerate() {
			assert_eq!(*result, Some(Some(vec![i as u8])));
		}
	}
}
