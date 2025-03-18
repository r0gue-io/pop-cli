// SPDX-License-Identifier: GPL-3.0

use crate::{Config, DefaultConfig, Error};
use keccak_hash::keccak;
use std::str::FromStr;
use subxt::{ext::subxt_core::utils::AccountId20, utils::to_hex};

/// Parses an account ID from its string representation.
///
/// # Arguments
/// * `account` - A string representing the account ID to parse.
pub fn parse_account(account: &str) -> Result<<DefaultConfig as Config>::AccountId, Error> {
	<DefaultConfig as Config>::AccountId::from_str(account)
		.map_err(|e| Error::AccountAddressParsing(format!("{}", e)))
}

/// Converts a list of accounts into EVM-compatible `AccountId20`.
///
/// # Arguments
/// * `accounts` - A vector of `AccountId32` strings.
pub fn convert_to_evm_accounts(accounts: Vec<String>) -> Result<Vec<String>, Error> {
	accounts
		.into_iter()
		.map(|account| {
			// Obtains the public address of the account by taking the last 20 bytes of the
			// Keccak-256 hash of the public key.
			let hash = keccak(parse_account(&account)?.0).0;
			let hash20 = hash[12..].try_into().expect("should be 20 bytes");
			let evm_account = AccountId20(hash20);
			Ok(to_hex(&evm_account.0))
		})
		.collect()
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
