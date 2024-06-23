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
