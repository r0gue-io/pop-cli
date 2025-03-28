// SPDX-License-Identifier: GPL-3.0

use crate::{Config, DefaultConfig, Error};
use sp_core::keccak_256;
use std::str::FromStr;
use subxt::utils::{to_hex, H160};

/// Parses an account ID from its string representation.
///
/// # Arguments
/// * `account` - A string representing the account ID to parse.
pub fn parse_account(account: &str) -> Result<<DefaultConfig as Config>::AccountId, Error> {
	<DefaultConfig as Config>::AccountId::from_str(account)
		.map_err(|e| Error::AccountAddressParsing(format!("{}", e)))
}

/// Parses a H160 account from its string representation.
///
/// # Arguments
/// * `account` - A hex-encoded string representation to parse.
pub fn parse_h160_account(account: &str) -> Result<H160, Error> {
	let bytes = contract_build::util::decode_hex(account)
		.map_err(|e| Error::AccountAddressParsing(format!("Invalid hex: {}", e)))?;

	if bytes.len() != 20 {
		return Err(Error::AccountAddressParsing(format!(
			"H160 must be 20 bytes in length, got {}",
			bytes.len()
		)));
	}
	Ok(H160::from_slice(&bytes[..]))
}

/// Converts a list of accounts into EVM-compatible `AccountId20`.
///
/// # Arguments
/// * `accounts` - A vector of `AccountId32` strings.
pub fn convert_to_evm_accounts(accounts: Vec<String>) -> Result<Vec<String>, Error> {
	accounts
		.into_iter()
		.map(|account| {
			let account_id = parse_account(&account)?.0;
			let evm_account = AccountIdMapper::to_address(&account_id);
			Ok(to_hex(evm_account))
		})
		.collect()
}

// Logic copied from `cargo-contract` for `AccountId` to `H160` mapping:
// https://github.com/use-ink/cargo-contract/blob/master/crates/extrinsics/src/lib.rs#L332
pub(crate) struct AccountIdMapper {}
impl AccountIdMapper {
	pub fn to_address(account_id: &[u8]) -> H160 {
		let mut account_bytes: [u8; 32] = [0u8; 32];
		account_bytes.copy_from_slice(&account_id[..32]);
		if Self::is_eth_derived(account_id) {
			// this was originally an eth address
			// we just strip the 0xEE suffix to get the original address
			H160::from_slice(&account_bytes[..20])
		} else {
			// this is an (ed|sr)25510 derived address
			// avoid truncating the public key by hashing it first
			let account_hash = keccak_256(account_bytes.as_ref());
			H160::from_slice(&account_hash[12..])
		}
	}

	/// Returns true if the passed account id is controlled by an Ethereum key.
	///
	/// This is a stateless check that just compares the last 12 bytes. Please note that
	/// it is theoretically possible to create an ed25519 keypair that passed this
	/// filter. However, this can't be used for an attack. It also won't happen by
	/// accident since everbody is using sr25519 where this is not a valid public key.
	//fn is_eth_derived(account_id: &[u8]) -> bool {
	fn is_eth_derived(account_bytes: &[u8]) -> bool {
		account_bytes[20..] == [0xEE; 12]
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[test]
	fn parse_account_works() -> Result<(), Error> {
		let account = parse_account("5CLPm1CeUvJhZ8GCDZCR7nWZ2m3XXe4X5MtAQK69zEjut36A")?;
		assert_eq!(account.to_string(), "5CLPm1CeUvJhZ8GCDZCR7nWZ2m3XXe4X5MtAQK69zEjut36A");
		Ok(())
	}

	#[test]
	fn parse_account_fails_wrong_value() -> Result<(), Error> {
		assert!(matches!(
			parse_account("5CLPm1CeUvJhZ8GCDZCR7"),
			Err(super::Error::AccountAddressParsing(..))
		));
		assert!(matches!(
			parse_account("wrongaccount"),
			Err(super::Error::AccountAddressParsing(..))
		));
		Ok(())
	}

	#[test]
	fn parse_h160_account_works() -> Result<(), Error> {
		let addr = "0x48550a4bb374727186c55365b7c9c0a1a31bdafe";
		let parsed = parse_h160_account(addr)?;
		assert_eq!(to_hex(parsed), addr.to_lowercase());
		Ok(())
	}

	#[test]
	fn parse_h160_account_fails_on_invalid_hex() -> Result<(), Error> {
		let invalid_hex = "wrongaccount";
		assert!(matches!(
			parse_h160_account(invalid_hex),
			Err(Error::AccountAddressParsing(msg)) if msg.contains("Invalid hex")
		));
		Ok(())
	}

	#[test]
	fn convert_to_evm_accounts_works() -> Result<()> {
		let accounts = vec![
			"5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY".to_string(),
			"5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty".to_string(),
		];
		let evm_accounts = convert_to_evm_accounts(accounts)?;
		assert_eq!(
			evm_accounts,
			vec![
				"0x9621dde636de098b43efb0fa9b61facfe328f99d".to_string(),
				"0x41dccbd49b26c50d34355ed86ff0fa9e489d1e01".to_string(),
			]
		);
		Ok(())
	}
}
