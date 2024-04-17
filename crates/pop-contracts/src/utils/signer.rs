use contract_build::util::decode_hex;
use sp_core::Bytes;
use subxt_signer::{sr25519::Keypair, SecretUri};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
	#[error("Failed to parse secret URI: {0}")]
	ParseSecretUriError(String),
	#[error("Failed to create keypair from URI: {0}")]
	KeyPairCreationError(String),
	#[error("Failed to parse hex encoded bytes: {0}")]
	HexParsingError(String),
}

/// Create a Signer from a secret URI.
pub(crate) fn create_signer(suri: &str) -> Result<Keypair, Error> {
	let uri = <SecretUri as std::str::FromStr>::from_str(suri)
		.map_err(|e| Error::ParseSecretUriError(format!("{}", e)))?;
	let keypair =
		Keypair::from_uri(&uri).map_err(|e| Error::KeyPairCreationError(format!("{}", e)))?;
	Ok(keypair)
}

/// Parse hex encoded bytes.
pub fn parse_hex_bytes(input: &str) -> Result<Bytes, Error> {
	let bytes = decode_hex(input).map_err(|e| Error::HexParsingError(format!("{}", e)))?;
	Ok(bytes.into())
}
