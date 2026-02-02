-- Create storage table
CREATE TABLE storage (
    block_hash BLOB NOT NULL,
    key BLOB NOT NULL,
    value BLOB,
    is_empty BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (block_hash, key)
);

-- Index to accelerate block-wide deletes/queries
CREATE INDEX idx_storage_block ON storage(block_hash);

-- Create blocks table
CREATE TABLE blocks (
    hash BLOB PRIMARY KEY NOT NULL,
    number BIGINT NOT NULL,
    parent_hash BLOB NOT NULL,
    header BLOB NOT NULL
);

-- Index to support lookups by number
CREATE INDEX idx_blocks_number ON blocks(number);

-- Create local keys table (stores unique keys that have been saved)
CREATE TABLE local_keys (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    key BLOB NOT NULL UNIQUE
);

-- Create local values table (stores temporal values for each key)
CREATE TABLE local_values (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    key_id INTEGER NOT NULL,
    value BLOB,
    valid_from BIGINT NOT NULL,
    valid_until BIGINT,
    FOREIGN KEY (key_id) REFERENCES local_keys(id)
);

-- Index to accelerate lookups by key_id and valid_from for range queries
-- Query pattern: WHERE key_id = ? AND valid_from <= X AND (valid_until IS NULL OR valid_until > X)
CREATE INDEX idx_local_values_key_validity ON local_values(key_id, valid_from);

-- Create prefix_scans table for tracking prefix scan progress
CREATE TABLE prefix_scans (
    block_hash BLOB NOT NULL,
    prefix BLOB NOT NULL,
    last_scanned_key BLOB,
    is_complete BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (block_hash, prefix)
);
