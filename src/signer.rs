use anyhow::Result;
use contract_build::util::decode_hex;
use sp_core::Bytes;
use subxt_signer::{sr25519::Keypair, SecretUri};

/// Create a Signer from a secret URI.
pub fn create_signer(suri: &str) -> Result<Keypair> {
	let uri = <SecretUri as std::str::FromStr>::from_str(suri)?;
	let keypair = Keypair::from_uri(&uri)?;
	Ok(keypair)
}
/// Parse hex encoded bytes.
pub(crate) fn parse_hex_bytes(input: &str) -> Result<Bytes> {
	let bytes = decode_hex(input)?;
	Ok(bytes.into())
}
