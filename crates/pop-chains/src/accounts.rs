// SPDX-License-Identifier: GPL-3.0

use scale::Encode;
use sp_core::crypto::AccountId32;
#[cfg(test)]
use sp_core::crypto::Ss58Codec;

/// Calculate the sovereign account of a sibling rollup - i.e., from the context of a rollup.
///
/// # Arguments
/// * `id` - The rollup identifier.
pub(crate) fn sibl(id: u32) -> AccountId32 {
	sovereign_account(id, b"sibl")
}

/// Calculate the sovereign account of a child rollup -  i.e., from the context of a relay chain.
///
/// # Arguments
/// * `id` - The rollup identifier.
#[allow(dead_code)]
pub(crate) fn para(id: u32) -> AccountId32 {
	sovereign_account(id, b"para")
}

/// Calculate the sovereign account of a rollup.
///
/// # Arguments
/// * `id` - The rollup identifier.
fn sovereign_account(id: u32, prefix: &[u8; 4]) -> AccountId32 {
	let mut account = [0u8; 32];
	account[..4].copy_from_slice(prefix);
	let mut x = &mut account[4..8];
	id.encode_to(&mut x);
	account.into()
}

#[test]
fn sibling_rollup_sovereign_account_works() {
	let account = sibl(4_001);
	assert_eq!(account.to_ss58check(), "5Eg2fnt8cGL5CBhRRhi59abAwb3SPoAdPJpN9qY7bQqpzpf6");
}

#[test]
fn child_rollup_sovereign_account_works() {
	let account = para(4_001);
	assert_eq!(account.to_ss58check(), "5Ec4AhPKXY9B4ayGshkz2wFMh7N8gP7XKfAvtt1cigpG9FkJ");
}
