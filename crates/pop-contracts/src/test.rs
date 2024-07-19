// SPDX-License-Identifier: GPL-3.0

use crate::{download_contracts_node, errors::Error};
use duct::cmd;
use flate2::read::GzDecoder;
use pop_common::GitHub;
use std::{
	env,
	env::consts::OS,
	fs::{self},
	io::{Seek, SeekFrom, Write},
	path::Path,
	path::PathBuf,
};
use tar::Archive;
use tempfile::tempfile;

/// Run unit tests of a smart contract.
///
/// # Arguments
///
/// * `path` - location of the smart contract.
pub fn test_smart_contract(path: Option<&Path>) -> Result<(), Error> {
	// Execute `cargo test` command in the specified directory.
	cmd("cargo", vec!["test"])
		.dir(path.unwrap_or_else(|| Path::new("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

const SUBSTRATE_CONTRACT_NODE: &str = "https://github.com/paritytech/substrate-contracts-node";
const BIN_NAME: &str = "substrate-contracts-node";
const STABLE_VERSION: &str = "v0.41.0";

/// Run e2e tests of a smart contract.
///
/// # Arguments
///
/// * `path` - location of the smart contract.
/// * `node` - location of the contracts node binary.
pub fn test_e2e_smart_contract(path: Option<&Path>, node: Option<&Path>) -> Result<(), Error> {
	// Set the environment variable `CONTRACTS_NODE` to the path of the contracts node.
	if let Some(node) = node {
		env::set_var("CONTRACTS_NODE", node);
	}
	// Execute `cargo test --features=e2e-tests` command in the specified directory.
	cmd("cargo", vec!["test", "--features=e2e-tests"])
		.dir(path.unwrap_or_else(|| Path::new("./")))
		.run()
		.map_err(|e| Error::TestCommand(format!("Cargo test command failed: {}", e)))?;
	Ok(())
}

/// Checks if the `substrate-contracts-node` binary exists
/// or if the binary exists in pop's cache.
/// returns:
/// - Some("") if the standalone binary exists
/// - Some(binary_cache_location) if the binary exists in pop's cache
/// - None if the binary does not exist
pub fn does_contracts_node_exist(cache: PathBuf) -> Option<PathBuf> {
	let cached_location = cache.join(BIN_NAME);
	println!("{:?}", cached_location);
	if cmd(BIN_NAME, vec!["--version"]).run().map_or(false, |_| true) {
		Some(PathBuf::new())
	} else if cached_location.exists() {
		Some(cached_location)
	} else {
		None
	}
}

// /// Downloads the latest contracts node binary
// pub async fn download_contracts_node(cache: PathBuf) -> Result<PathBuf, Error> {
// 	let cached_file = cache.join(BIN_NAME);
// 	if !cached_file.exists() {
// 		let archive = archive_name_by_target()?;

// 		let latest_version = latest_contract_node_release().await?;
// 		let releases_url =
// 			format!("{SUBSTRATE_CONTRACT_NODE}/releases/download/{latest_version}/{archive}");
// 		// Download archive
// 		let response = reqwest::get(releases_url.as_str()).await?.error_for_status()?;
// 		let mut file = tempfile()?;
// 		file.write_all(&response.bytes().await?)?;
// 		file.seek(SeekFrom::Start(0))?;
// 		// Extract contents
// 		let tar = GzDecoder::new(file);
// 		let mut archive = Archive::new(tar);
// 		archive.unpack(cache.clone())?;
// 		// Copy the file into the cache folder and remove the folder artifacts
// 		let extracted_dir = cache.join(release_folder_by_target()?);
// 		fs::copy(&extracted_dir.join(BIN_NAME), &cached_file)?;
// 		fs::remove_dir_all(&extracted_dir.parent().unwrap_or(&cache.join("artifacts")))?;
// 	}

// 	Ok(cached_file)
// }

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile;

	#[test]
	fn test_smart_contract_works() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
		// Run unit tests for the smart contract in the temporary contract directory.
		test_smart_contract(Some(&temp_dir.path().join("test_contract")))?;
		Ok(())
	}

	#[test]
	fn test_smart_contract_wrong_folder() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		assert!(matches!(
			test_smart_contract(Some(&temp_dir.path().join(""))),
			Err(Error::TestCommand(..))
		));
		Ok(())
	}

	#[test]
	fn test_e2e_smart_contract_set_env_variable() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
		// Ignore 2e2 testing in this scenario, will fail. Only test if the environment variable CONTRACTS_NODE is set.
		let err = test_e2e_smart_contract(Some(&temp_dir.path().join("test_contract")), None);
		assert!(err.is_err());
		// The environment variable `CONTRACTS_NODE` should not be set.
		assert!(env::var("CONTRACTS_NODE").is_err());
		let err = test_e2e_smart_contract(
			Some(&temp_dir.path().join("test_contract")),
			Some(&Path::new("/path/to/contracts-node")),
		);
		assert!(err.is_err());
		// The environment variable `CONTRACTS_NODE` should has been set.
		assert_eq!(
			env::var("CONTRACTS_NODE").unwrap(),
			Path::new("/path/to/contracts-node").display().to_string()
		);
		Ok(())
	}

	#[test]
	fn test_e2e_smart_contract_fails_no_e2e_tests() -> Result<(), Error> {
		let temp_dir = tempfile::tempdir()?;
		cmd("cargo", ["new", "test_contract", "--bin"]).dir(temp_dir.path()).run()?;
		assert!(matches!(
			test_e2e_smart_contract(Some(&temp_dir.path().join("test_contract")), None),
			Err(Error::TestCommand(..))
		));
		Ok(())
	}
}
