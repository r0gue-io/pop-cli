// SPDX-License-Identifier: GPL-3.0

use crate::errors::Error;
use contract_build::{util::decode_hex, ManifestPath};
use contract_extrinsics::BalanceVariant;
use ink_env::{DefaultEnvironment, Environment};
use pop_common::{Config, DefaultConfig};
use sp_core::Bytes;
use std::{
	path::{Path, PathBuf},
	str::FromStr,
};

pub mod metadata;

/// Retrieves the manifest path for a contract project.
///
/// # Arguments
/// * `path` - An optional path to the project directory.
pub fn get_manifest_path(path: Option<&Path>) -> Result<ManifestPath, Error> {
	if let Some(path) = path {
		let full_path = PathBuf::from(path.to_string_lossy().to_string() + "/Cargo.toml");
		ManifestPath::try_from(Some(full_path))
			.map_err(|e| Error::ManifestPath(format!("Failed to get manifest path: {}", e)))
	} else {
		ManifestPath::try_from(path.as_ref())
			.map_err(|e| Error::ManifestPath(format!("Failed to get manifest path: {}", e)))
	}
}

/// Parses a balance value from a string representation.
///
/// # Arguments
/// * `balance` - A string representing the balance value to parse.
pub fn parse_balance(
	balance: &str,
) -> Result<BalanceVariant<<DefaultEnvironment as Environment>::Balance>, Error> {
	BalanceVariant::from_str(balance).map_err(|e| Error::BalanceParsing(format!("{}", e)))
}

/// Parses an account ID from its string representation.
///
/// # Arguments
/// * `account` - A string representing the account ID to parse.
pub fn parse_account(account: &str) -> Result<<DefaultConfig as Config>::AccountId, Error> {
	<DefaultConfig as Config>::AccountId::from_str(account)
		.map_err(|e| Error::AccountAddressParsing(format!("{}", e)))
}

/// Parse hex encoded bytes.
///
/// # Arguments
/// * `input` - A string containing hex-encoded bytes.
pub fn parse_hex_bytes(input: &str) -> Result<Bytes, Error> {
	let bytes = decode_hex(input).map_err(|e| Error::HexParsing(format!("{}", e)))?;
	Ok(bytes.into())
}

/// Canonicalizes the given path to ensure consistency and resolve any symbolic links.
///
/// # Arguments
/// * `target` - A reference to the `Path` to be canonicalized.
pub fn canonicalized_path(target: &Path) -> Result<PathBuf, Error> {
	// Canonicalize the target path to ensure consistency and resolve any symbolic links.
	target
		.canonicalize()
		// If an I/O error occurs during canonicalization, convert it into an Error enum variant.
		.map_err(Error::IO)
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;
	use std::fs;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		crate::create_smart_contract(
			"test_contract",
			temp_contract_dir.as_path(),
			&crate::Contract::Standard,
		)?;
		Ok(temp_dir)
	}

	#[test]
	fn test_get_manifest_path() -> Result<(), Error> {
		let temp_dir = setup_test_environment()?;
		get_manifest_path(Some(&PathBuf::from(temp_dir.path().join("test_contract"))))?;
		Ok(())
	}

	#[test]
	fn test_canonicalized_path() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		// Error case
		let error_directory = canonicalized_path(&temp_dir.path().join("my_directory"));
		assert!(error_directory.is_err());
		// Success case
		canonicalized_path(temp_dir.path())?;
		Ok(())
	}

	#[test]
	fn parse_balance_works() -> Result<(), Error> {
		let balance = parse_balance("100000")?;
		assert_eq!(balance, BalanceVariant::Default(100000));
		Ok(())
	}

	#[test]
	fn parse_balance_fails_wrong_balance() -> Result<(), Error> {
		assert!(matches!(parse_balance("wrongbalance"), Err(super::Error::BalanceParsing(..))));
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

	#[test]
	fn parse_hex_bytes_works() -> Result<(), Error> {
		let input_in_hex = "48656c6c6f";
		let result = parse_hex_bytes(input_in_hex)?;
		assert_eq!(result, Bytes(vec![72, 101, 108, 108, 111]));
		Ok(())
	}

	#[test]
	fn parse_hex_bytes_fails_wrong_input() -> Result<()> {
		assert!(matches!(parse_hex_bytes("wronghexvalue"), Err(Error::HexParsing(..))));
		Ok(())
	}
}
