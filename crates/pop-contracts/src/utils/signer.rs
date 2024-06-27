// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use contract_build::util::decode_hex;
use sp_core::Bytes;
use subxt_signer::{sr25519::Keypair, SecretUri};

/// Create a Signer from a secret URI.
pub(crate) fn create_signer(suri: &str) -> Result<Keypair, Error> {
	let uri = <SecretUri as std::str::FromStr>::from_str(suri)
		.map_err(|e| Error::ParseSecretURI(format!("{}", e)))?;
	let keypair = Keypair::from_uri(&uri).map_err(|e| Error::KeyPairCreation(format!("{}", e)))?;
	Ok(keypair)
}

/// Parse hex encoded bytes.
pub fn parse_hex_bytes(input: &str) -> Result<Bytes, Error> {
	let bytes = decode_hex(input).map_err(|e| Error::HexParsing(format!("{}", e)))?;
	Ok(bytes.into())
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
	fn parse_hex_bytes_works() -> Result<(), Error> {
		let input_in_hex = "48656c6c6f";
		let result = parse_hex_bytes(input_in_hex)?;
		assert_eq!(result, Bytes(vec![72, 101, 108, 108, 111]));
		Ok(())
	}

	#[test]
	fn parse_hex_bytes_fails_wrong_input() -> Result<(), Error> {
		assert!(matches!(parse_hex_bytes("wronghexvalue"), Err(Error::HexParsing(..))));
		Ok(())
	}
}
