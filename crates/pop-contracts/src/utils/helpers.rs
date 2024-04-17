// SPDX-License-Identifier: GPL-3.0
use anyhow::{anyhow, Result};
use contract_build::ManifestPath;
use contract_extrinsics::BalanceVariant;
use ink_env::{DefaultEnvironment, Environment};
use std::{path::PathBuf, str::FromStr};
use subxt::{Config, PolkadotConfig as DefaultConfig};

// If the user specifies a path (which is not the current directory), it will have to manually
// add a Cargo.toml file. If not provided, pop-cli will ask the user for a specific path. or ask
// to the user the specific path (Like cargo-contract does)
pub fn get_manifest_path(path: &Option<PathBuf>) -> anyhow::Result<ManifestPath> {
	if path.is_some() {
		let full_path: PathBuf =
			PathBuf::from(path.as_ref().unwrap().to_string_lossy().to_string() + "/Cargo.toml");

		return ManifestPath::try_from(Some(full_path));
	} else {
		return ManifestPath::try_from(path.as_ref());
	}
}

/// Parse a balance from string format
pub fn parse_balance(
	balance: &str,
) -> Result<BalanceVariant<<DefaultEnvironment as Environment>::Balance>> {
	BalanceVariant::from_str(balance).map_err(|e| anyhow!("Balance parsing failed: {e}"))
}
pub fn parse_account(account: &str) -> Result<<DefaultConfig as Config>::AccountId> {
	<DefaultConfig as Config>::AccountId::from_str(account)
		.map_err(|e| anyhow::anyhow!("Account address parsing failed: {e}"))
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
