-- Delete storage table
DROP TABLE storage;

-- Delete blocks table
DROP TABLE blocks;

-- Delete local values table (must be dropped before local_keys due to FK)
DROP TABLE local_values;

-- Delete local keys table
DROP TABLE local_keys;

-- Delete prefix_scans table
DROP TABLE prefix_scans;
