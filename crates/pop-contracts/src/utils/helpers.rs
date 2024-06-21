// SPDX-License-Identifier: GPL-3.0
use crate::errors::Error;
use contract_build::ManifestPath;
use contract_extrinsics::BalanceVariant;
use ink_env::{DefaultEnvironment, Environment};
use std::{
	collections::HashMap,
	fs,
	io::{Read, Write},
	path::{Path, PathBuf},
	str::FromStr,
};
use subxt::{Config, PolkadotConfig as DefaultConfig};

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

pub fn canonicalized_path(target: &Path) -> Result<PathBuf, Error> {
	// Canonicalize the target path to ensure consistency and resolve any symbolic links.
	target
		.canonicalize()
		// If an I/O error occurs during canonicalization, convert it into an Error enum variant.
		.map_err(|e| Error::IO(e))
}

pub fn replace_in_file(file_path: PathBuf, replacements: HashMap<&str, &str>) -> Result<(), Error> {
	// Read the file content
	let mut file_content = String::new();
	fs::File::open(&file_path)?.read_to_string(&mut file_content)?;
	// Perform the replacements
	let mut modified_content = file_content;
	for (target, replacement) in &replacements {
		modified_content = modified_content.replace(target, replacement);
	}
	// Write the modified content back to the file
	let mut file = fs::File::create(&file_path)?;
	file.write_all(modified_content.as_bytes())?;
	Ok(())
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
		crate::create_smart_contract(
			"test_contract",
			temp_contract_dir.as_path(),
			&crate::Template::Standard,
		)?;
		Ok(temp_dir)
	}

	#[test]
	fn test_get_manifest_path() -> Result<(), Error> {
		let temp_dir = setup_test_environment()?;
		get_manifest_path(&Some(PathBuf::from(temp_dir.path().join("test_contract"))))?;
		Ok(())
	}
}
