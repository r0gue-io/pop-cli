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

-- Create local storage table
CREATE TABLE local_storage (
    block_number BIGINT NOT NULL,
    key BLOB NOT NULL,
    value BLOB,
    is_empty BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (block_number, key)
);

-- Index to accelerate block-wide deletes/queries
CREATE INDEX idx_local_storage_block ON local_storage(block_number);

-- Create prefix_scans table for tracking prefix scan progress
CREATE TABLE prefix_scans (
    block_hash BLOB NOT NULL,
    prefix BLOB NOT NULL,
    last_scanned_key BLOB,
    is_complete BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (block_hash, prefix)
);
