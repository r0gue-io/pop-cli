// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use subxt_signer::{sr25519::Keypair, SecretUri};

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
}
