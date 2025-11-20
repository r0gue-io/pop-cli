// SPDX-License-Identifier: GPL-3.0

use crate::{DefaultConfig, errors::Error};
use sp_core::sr25519::Signature;
use subxt::{Config, tx::Signer, utils::AccountId32};
use subxt_signer::{
	SecretUri,
	sr25519::{Keypair, Signature as SubxtSr25519Signature},
};

/// A signer that delegates signing to an external function.
#[derive(Debug, Clone)]
pub struct RemoteSigner {
	url: String,
	sign_fn: fn(&str, &[u8]) -> Signature,
}

impl<T> Signer<T> for RemoteSigner
where
	T: Config,
	T::AccountId: From<AccountId32>,
	T::Signature: From<SubxtSr25519Signature>,
{
	fn account_id(&self) -> T::AccountId {
		// Dummy value
		AccountId32::from([1u8; 32]).into()
	}

	fn sign(&self, signer_payload: &[u8]) -> T::Signature {
		let sp_sig = (self.sign_fn)(&self.url, signer_payload);
		let mut bytes = [0u8; 64];
		bytes.copy_from_slice(sp_sig.as_ref());
		SubxtSr25519Signature(bytes).into()
	}
}

/// Signer that can be used for both local and remote signing.
#[derive(Clone, Debug)]
pub enum AnySigner {
	/// Signer for local accounts.
	Local(Keypair),
	/// Signer for remote accounts.
	Remote(RemoteSigner),
}

impl Signer<DefaultConfig> for AnySigner {
	fn account_id(&self) -> <DefaultConfig as Config>::AccountId {
		match self {
			AnySigner::Local(kp) => <Keypair as Signer<DefaultConfig>>::account_id(kp),
			AnySigner::Remote(rs) => <RemoteSigner as Signer<DefaultConfig>>::account_id(rs),
		}
	}

	fn sign(&self, signer_payload: &[u8]) -> <DefaultConfig as Config>::Signature {
		match self {
			AnySigner::Local(kp) =>
				<Keypair as Signer<DefaultConfig>>::sign(kp, signer_payload).into(),
			AnySigner::Remote(rs) =>
				<RemoteSigner as Signer<DefaultConfig>>::sign(rs, signer_payload),
		}
	}
}

/// Create a remote signer that delegates signing to an external function.
///
/// # Arguments
/// - `url`: The endpoint the remote signer should use.
/// - `sign_fn`: A function that, given the url and payload, returns an sr25519 signature.
pub fn create_remote_signer(url: &str, sign_fn: fn(&str, &[u8]) -> Signature) -> RemoteSigner {
	RemoteSigner { url: url.to_string(), sign_fn }
}

/// Create a keypair from a secret URI.
///
/// # Arguments
/// `suri` - Secret URI string used to generate the `Keypair`.
pub fn create_local_signer(suri: &str) -> anyhow::Result<Keypair> {
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
		let keypair = create_local_signer("//Alice")?;
		assert_eq!(
			keypair.public_key().to_account_id().to_string(),
			"5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY" //Alice account
		);
		Ok(())
	}

	#[test]
	fn create_signer_fails_wrong_key() -> Result<(), Error> {
		assert!(matches!(create_local_signer("11111"), Err(Error::KeyPairCreation(..))));
		Ok(())
	}
}
