use contract_build::ManifestPath;
use contract_extrinsics::BalanceVariant;
use ink_env::{DefaultEnvironment, Environment};
use std::{path::PathBuf, str::FromStr};
use subxt::{Config, PolkadotConfig as DefaultConfig};
use crate::errors::Error;

pub fn get_manifest_path(path: &Option<PathBuf>) -> Result<ManifestPath, Error> {
	if let Some(path) = path {
		let full_path = PathBuf::from(path.to_string_lossy().to_string() + "/Cargo.toml");
		return ManifestPath::try_from(Some(full_path))
			.map_err(|e| Error::ManifestPath(format!("Failed to get manifest path: {}", e)));
	} else {
		return ManifestPath::try_from(path.as_ref())
			.map_err(|e| Error::ManifestPath(format!("Failed to get manifest path: {}", e)));
	}
}

pub fn parse_balance(
	balance: &str,
) -> Result<BalanceVariant<<DefaultEnvironment as Environment>::Balance>, Error> {
	BalanceVariant::from_str(balance).map_err(|e| Error::BalanceParsing(format!("{}", e)))
}

pub fn parse_account(account: &str) -> Result<<DefaultConfig as Config>::AccountId, Error> {
	<DefaultConfig as Config>::AccountId::from_str(account)
		.map_err(|e| Error::AccountAddressParsing(format!("{}", e)))
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::fs;

	fn setup_test_environment() -> Result<tempfile::TempDir, Error> {
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let temp_contract_dir = temp_dir.path().join("test_contract");
		fs::create_dir(&temp_contract_dir)?;
		let result =
			crate::create_smart_contract("test_contract".to_string(), temp_contract_dir.as_path());
		assert!(result.is_ok(), "Contract test environment setup failed");

		Ok(temp_dir)
	}

	#[test]
	fn test_get_manifest_path() -> Result<(), Error> {
		let temp_dir = setup_test_environment()?;
		let manifest_path =
			get_manifest_path(&Some(PathBuf::from(temp_dir.path().join("test_contract"))));
		assert!(manifest_path.is_ok());
		Ok(())
	}
}
