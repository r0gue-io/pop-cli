// SPDX-License-Identifier: GPL-3.0

//! Dev account utilities for forked chains.
//!
//! Provides well-known dev account public keys and helpers for building storage
//! entries to fund them and set sudo.
//!
//! Two account sets are supported:
//! - **Substrate** (32-byte sr25519): Alice, Bob, Charlie, Dave, Eve, Ferdie
//! - **Ethereum** (20-byte H160): Alith, Baltathar, Charleth, Dorothy, Ethan, Faith

use crate::strings::rpc_server::storage;

/// Default balance for dev accounts: half of u128::MAX.
pub const DEV_BALANCE: u128 = u128::MAX / 2;

// ---------------------------------------------------------------------------
// Substrate dev accounts (32-byte sr25519 public keys)
// ---------------------------------------------------------------------------

/// Well-known dev account: Alice.
pub const ALICE: [u8; 32] = [
	0xd4, 0x35, 0x93, 0xc7, 0x15, 0xfd, 0xd3, 0x1c, 0x61, 0x14, 0x1a, 0xbd, 0x04, 0xa9, 0x9f, 0xd6,
	0x82, 0x2c, 0x85, 0x58, 0x85, 0x4c, 0xcd, 0xe3, 0x9a, 0x56, 0x84, 0xe7, 0xa5, 0x6d, 0xa2, 0x7d,
];

/// Well-known dev account: Bob.
pub const BOB: [u8; 32] = [
	0x8e, 0xaf, 0x04, 0x15, 0x16, 0x87, 0x73, 0x63, 0x26, 0xc9, 0xfe, 0xa1, 0x7e, 0x25, 0xfc, 0x52,
	0x87, 0x61, 0x36, 0x93, 0xc9, 0x12, 0x90, 0x9c, 0xb2, 0x26, 0xaa, 0x47, 0x94, 0xf2, 0x6a, 0x48,
];

/// Well-known dev account: Charlie.
pub const CHARLIE: [u8; 32] = [
	0x90, 0xb5, 0xab, 0x20, 0x5c, 0x69, 0x74, 0xc9, 0xea, 0x84, 0x1b, 0xe6, 0x88, 0x86, 0x46, 0x33,
	0xdc, 0x9c, 0xa8, 0xa3, 0x57, 0x84, 0x3e, 0xea, 0xcf, 0x23, 0x14, 0x64, 0x99, 0x65, 0xfe, 0x22,
];

/// Well-known dev account: Dave.
pub const DAVE: [u8; 32] = [
	0x30, 0x67, 0x21, 0x21, 0x1d, 0x54, 0x04, 0xbd, 0x9d, 0xa8, 0x8e, 0x02, 0x04, 0x36, 0x0a, 0x1a,
	0x9a, 0xb8, 0xb8, 0x7c, 0x66, 0xc1, 0xbc, 0x2f, 0xcd, 0xd3, 0x7f, 0x3c, 0x22, 0x22, 0xcc, 0x20,
];

/// Well-known dev account: Eve.
pub const EVE: [u8; 32] = [
	0xe6, 0x59, 0xa7, 0xa1, 0x62, 0x8c, 0xdd, 0x93, 0xfe, 0xbc, 0x04, 0xa4, 0xe0, 0x64, 0x6e, 0xa2,
	0x0e, 0x9f, 0x5f, 0x0c, 0xe0, 0x97, 0xd9, 0xa0, 0x52, 0x90, 0xd4, 0xa9, 0xe0, 0x54, 0xdf, 0x4e,
];

/// Well-known dev account: Ferdie.
pub const FERDIE: [u8; 32] = [
	0x1c, 0xbd, 0x2d, 0x43, 0x53, 0x0a, 0x44, 0x70, 0x5a, 0xd0, 0x88, 0xaf, 0x31, 0x3e, 0x18, 0xf8,
	0x0b, 0x53, 0xef, 0x16, 0xb3, 0x61, 0x77, 0xcd, 0x4b, 0x77, 0xb8, 0x46, 0xf2, 0xa5, 0xf0, 0x7c,
];

/// All Substrate dev accounts (name, 32-byte public key).
pub const SUBSTRATE_DEV_ACCOUNTS: [(&str, [u8; 32]); 6] = [
	("Alice", ALICE),
	("Bob", BOB),
	("Charlie", CHARLIE),
	("Dave", DAVE),
	("Eve", EVE),
	("Ferdie", FERDIE),
];

// ---------------------------------------------------------------------------
// Ethereum dev accounts (20-byte H160 addresses, from Frontier/Moonbeam)
// ---------------------------------------------------------------------------

/// Well-known Ethereum dev account: Alith.
pub const ALITH: [u8; 20] = [
	0xf2, 0x4f, 0xf3, 0xa9, 0xcf, 0x04, 0xc7, 0x1d, 0xbc, 0x94, 0xd0, 0xb5, 0x66, 0xf7, 0xa2, 0x7b,
	0x94, 0x56, 0x6c, 0xac,
];

/// Well-known Ethereum dev account: Baltathar.
pub const BALTATHAR: [u8; 20] = [
	0x3c, 0xd0, 0xa7, 0x05, 0xa2, 0xdc, 0x65, 0xe5, 0xb1, 0xe1, 0x20, 0x58, 0x96, 0xba, 0xa2, 0xbe,
	0x8a, 0x07, 0xc6, 0xe0,
];

/// Well-known Ethereum dev account: Charleth.
pub const CHARLETH: [u8; 20] = [
	0x79, 0x8d, 0x4b, 0xa9, 0xba, 0xf0, 0x06, 0x4e, 0xc1, 0x9e, 0xb4, 0xf0, 0xa1, 0xa4, 0x57, 0x85,
	0xae, 0x9d, 0x6d, 0xfc,
];

/// Well-known Ethereum dev account: Dorothy.
pub const DOROTHY: [u8; 20] = [
	0x77, 0x35, 0x39, 0xd4, 0xac, 0x0e, 0x78, 0x62, 0x33, 0xd9, 0x0a, 0x23, 0x36, 0x54, 0xcc, 0xee,
	0x26, 0xa6, 0x13, 0xd9,
];

/// Well-known Ethereum dev account: Ethan.
pub const ETHAN: [u8; 20] = [
	0xff, 0x64, 0xd3, 0xf6, 0xef, 0xe2, 0x31, 0x7e, 0xe2, 0x80, 0x7d, 0x22, 0x3a, 0x0b, 0xdc, 0x4c,
	0x0c, 0x49, 0xdf, 0xdb,
];

/// Well-known Ethereum dev account: Faith.
pub const FAITH: [u8; 20] = [
	0xc0, 0xf0, 0xf4, 0xab, 0x32, 0x4c, 0x46, 0xe5, 0x5d, 0x02, 0xd0, 0x03, 0x33, 0x43, 0xb4, 0xbe,
	0x8a, 0x55, 0x53, 0x2d,
];

/// All Ethereum dev accounts (name, 20-byte H160 address).
pub const ETHEREUM_DEV_ACCOUNTS: [(&str, [u8; 20]); 6] = [
	("Alith", ALITH),
	("Baltathar", BALTATHAR),
	("Charleth", CHARLETH),
	("Dorothy", DOROTHY),
	("Ethan", ETHAN),
	("Faith", FAITH),
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert an Ethereum address into the AccountId32 fallback format used by
/// `pallet-revive::AccountId32Mapper`: `address(20 bytes) ++ 0xEE * 12`.
pub fn ethereum_fallback_account_id(address: &[u8; 20]) -> [u8; 32] {
	let mut account_id = [0xEE; 32];
	account_id[..20].copy_from_slice(address);
	account_id
}

/// Compute the `System::Account` storage key for an account (Blake2_128Concat).
///
/// Works with both 32-byte (Substrate) and 20-byte (Ethereum) account IDs.
pub fn account_storage_key(account: &[u8]) -> Vec<u8> {
	let mut key = Vec::new();
	key.extend(sp_core::twox_128(storage::SYSTEM_PALLET));
	key.extend(sp_core::twox_128(storage::ACCOUNT_STORAGE));
	key.extend(sp_core::blake2_128(account));
	key.extend(account);
	key
}

/// Compute the `Sudo::Key` storage key.
pub fn sudo_key_storage_key() -> Vec<u8> {
	let mut key = Vec::new();
	key.extend(sp_core::twox_128(storage::SUDO_PALLET));
	key.extend(sp_core::twox_128(storage::SUDO_KEY_STORAGE));
	key
}

/// Build a fresh `AccountInfo` with the given free balance.
///
/// Layout (80 bytes):
/// - nonce: u32 (4 bytes) = 0
/// - consumers: u32 (4 bytes) = 0
/// - providers: u32 (4 bytes) = 1
/// - sufficients: u32 (4 bytes) = 0
/// - data.free: u128 (16 bytes)
/// - data.reserved: u128 (16 bytes) = 0
/// - data.frozen: u128 (16 bytes) = 0
/// - data.flags: u128 (16 bytes) = 0
pub fn build_account_info(free_balance: u128) -> Vec<u8> {
	let mut data = vec![0u8; 80];
	// providers = 1 (offset 8..12)
	data[8..12].copy_from_slice(&1u32.to_le_bytes());
	// data.free (offset 16..32)
	data[16..32].copy_from_slice(&free_balance.to_le_bytes());
	data
}

/// Patch the free balance in an existing `AccountInfo` blob.
///
/// Overwrites bytes 16..32 (the `data.free` field) with the new balance.
pub fn patch_free_balance(existing: &[u8], new_balance: u128) -> Vec<u8> {
	let mut patched = existing.to_vec();
	patched[16..32].copy_from_slice(&new_balance.to_le_bytes());
	patched
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn substrate_account_storage_key_has_correct_length() {
		let key = account_storage_key(&ALICE);
		// twox128("System") + twox128("Account") + blake2_128(account) + account
		// = 16 + 16 + 16 + 32 = 80
		assert_eq!(key.len(), 80);
	}

	#[test]
	fn ethereum_account_storage_key_has_correct_length() {
		let key = account_storage_key(&ALITH);
		// twox128("System") + twox128("Account") + blake2_128(account) + account
		// = 16 + 16 + 16 + 20 = 68
		assert_eq!(key.len(), 68);
	}

	#[test]
	fn ethereum_fallback_account_id_has_expected_shape() {
		let fallback = ethereum_fallback_account_id(&ALITH);
		assert_eq!(&fallback[..20], &ALITH);
		assert!(fallback[20..].iter().all(|b| *b == 0xEE));
	}

	#[test]
	fn ethereum_fallback_account_storage_key_has_correct_length() {
		let fallback = ethereum_fallback_account_id(&ALITH);
		let key = account_storage_key(&fallback);
		// twox128("System") + twox128("Account") + blake2_128(account) + account
		// = 16 + 16 + 16 + 32 = 80
		assert_eq!(key.len(), 80);
	}

	#[test]
	fn sudo_key_storage_key_has_correct_length() {
		let key = sudo_key_storage_key();
		// twox128("Sudo") + twox128("Key") = 16 + 16 = 32
		assert_eq!(key.len(), 32);
	}

	#[test]
	fn build_account_info_sets_providers_and_balance() {
		let balance: u128 = 1_000_000_000_000;
		let info = build_account_info(balance);
		assert_eq!(info.len(), 80);
		// providers = 1
		assert_eq!(u32::from_le_bytes(info[8..12].try_into().unwrap()), 1);
		// free balance
		assert_eq!(u128::from_le_bytes(info[16..32].try_into().unwrap()), balance);
	}

	#[test]
	fn patch_free_balance_preserves_other_fields() {
		let original = build_account_info(100);
		let patched = patch_free_balance(&original, 999);
		// Balance changed
		assert_eq!(u128::from_le_bytes(patched[16..32].try_into().unwrap()), 999);
		// Providers preserved
		assert_eq!(u32::from_le_bytes(patched[8..12].try_into().unwrap()), 1);
		// Length preserved
		assert_eq!(patched.len(), 80);
	}
}
