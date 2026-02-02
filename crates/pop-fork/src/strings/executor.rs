// SPDX-License-Identifier: GPL-3.0

//! String constants for the executor module.

/// Well-known storage key prefixes.
pub mod storage_prefixes {
	/// Default prefix for child storage keys in the main trie.
	///
	/// Child storage tries are stored in the main trie under keys prefixed with this value,
	/// followed by the child trie identifier and the actual storage key.
	pub const DEFAULT_CHILD_STORAGE: &[u8] = b":child_storage:default:";
}

/// Magic byte sequences for signature mocking in tests.
pub mod magic_signature {
	/// Magic bytes that identify a mock signature.
	///
	/// Signatures starting with these bytes are recognized as test signatures
	/// when signature mocking is enabled.
	pub const PREFIX: &[u8] = &[0xde, 0xad, 0xbe, 0xef];

	/// Padding byte used to fill the rest of a magic signature.
	pub const PADDING: u8 = 0xcd;
}
