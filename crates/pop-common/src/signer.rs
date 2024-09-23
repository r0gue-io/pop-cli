// SPDX-License-Identifier: GPL-3.0

use crate::{errors::Error, Config, DefaultConfig};
use std::str::FromStr;
use subxt_signer::{sr25519::Keypair, SecretUri};

pub fn parse_account(account: &str) -> Result<<DefaultConfig as Config>::AccountId, Error> {
	<DefaultConfig as Config>::AccountId::from_str(account)
		.map_err(|e| Error::AccountAddressParsing(format!("{}", e)))
}

/// Create a Signer from a secret URI.
pub fn create_signer(suri: &str) -> Result<Keypair, Error> {
	let uri = <SecretUri as std::str::FromStr>::from_str(suri)
		.map_err(|e| Error::ParseSecretURI(format!("{}", e)))?;
	let keypair = Keypair::from_uri(&uri).map_err(|e| Error::KeyPairCreation(format!("{}", e)))?;
	Ok(keypair)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[test]
	fn create_signer_works() -> Result<(), Error> {
		let keypair = create_signer("//Alice")?;
		assert_eq!(
			keypair.public_key().to_account_id().to_string(),
			"5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY" //Alice account
		);
		Ok(())
	}

	#[test]
	fn create_signer_fails_wrong_key() -> Result<(), Error> {
		assert!(matches!(create_signer("11111"), Err(Error::KeyPairCreation(..))));
		Ok(())
	}

	#[test]
	fn parse_account_works() -> Result<(), Error> {
		let account = parse_account("5CLPm1CeUvJhZ8GCDZCR7nWZ2m3XXe4X5MtAQK69zEjut36A")?;
		assert_eq!(account.to_string(), "5CLPm1CeUvJhZ8GCDZCR7nWZ2m3XXe4X5MtAQK69zEjut36A");
		Ok(())
	}

	#[test]
	fn parse_account_fails_wrong_value() -> Result<(), Error> {
		assert!(matches!(
			parse_account("wrongaccount"),
			Err(super::Error::AccountAddressParsing(..))
		));
		Ok(())
	}
}
