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
