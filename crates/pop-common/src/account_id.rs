// SPDX-License-Identifier: GPL-3.0

use crate::{Config, DefaultConfig, Error};
use std::str::FromStr;
use subxt::utils::H160;

/// Parses a Substrate account ID from its string representation.
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
}
