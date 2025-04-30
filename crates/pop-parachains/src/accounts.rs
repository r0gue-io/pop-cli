// SPDX-License-Identifier: GPL-3.0

use scale::Encode;
use sp_core::crypto::AccountId32;
#[cfg(test)]
use sp_core::crypto::Ss58Codec;

pub(crate) fn sibl(para_id: u32) -> AccountId32 {
	sovereign_account(para_id, b"sibl")
}

#[allow(dead_code)]
pub(crate) fn para(para_id: u32) -> AccountId32 {
	sovereign_account(para_id, b"para")
}

fn sovereign_account(para_id: u32, prefix: &[u8; 4]) -> AccountId32 {
	let mut account = [0u8; 32];
	account[..4].copy_from_slice(prefix);
	let mut x = &mut account[4..8];
	para_id.encode_to(&mut x);
	account.into()
}

#[test]
fn sibling_parachain_sovereign_account_works() {
	let account = sibl(4_001);
	assert_eq!(account.to_ss58check(), "5Eg2fnt8cGL5CBhRRhi59abAwb3SPoAdPJpN9qY7bQqpzpf6");
}

#[test]
fn child_parachain_sovereign_account_works() {
	let account = para(4_001);
	assert_eq!(account.to_ss58check(), "5Ec4AhPKXY9B4ayGshkz2wFMh7N8gP7XKfAvtt1cigpG9FkJ");
}
