use duct::cmd;
use flate2::read::GzDecoder;
use std::{
	env::consts::OS,
	io::{Seek, SeekFrom, Write},
	path::PathBuf,
};
use tar::Archive;
use tempfile::tempfile;

use crate::{errors::Error, utils::git::GitHub};

const SUBSTRATE_CONTRACT_NODE: &str = "https://github.com/paritytech/substrate-contracts-node";
const BIN_NAME: &str = "substrate-contracts-node";
const STABLE_VERSION: &str = "v0.41.0";

pub async fn run_contracts_node(cache: PathBuf) -> Result<(), Error> {
	let cached_file = cache.join(release_folder_by_target()?).join(BIN_NAME);
	if !cached_file.exists() {
		let archive = archive_name_by_target()?;

		let latest_version = latest_contract_node_release().await?;
		let releases_url =
			format!("{SUBSTRATE_CONTRACT_NODE}/releases/download/{latest_version}/{archive}");
		// Download archive
		let response = reqwest::get(releases_url.as_str()).await?.error_for_status()?;
		let mut file = tempfile()?;
		file.write_all(&response.bytes().await?)?;
		file.seek(SeekFrom::Start(0))?;
		// Extract contents
		let tar = GzDecoder::new(file);
		let mut archive = Archive::new(tar);
		archive.unpack(cache.clone())?;
	}
	cmd(cached_file.display().to_string().as_str(), Vec::<&str>::new())
		.run()
		.map_err(|_e| return Error::UpContractsNode(BIN_NAME.to_string()))?;
	Ok(())
}

async fn latest_contract_node_release() -> Result<String, Error> {
	let repo = GitHub::parse(SUBSTRATE_CONTRACT_NODE)?;
	match repo.get_latest_releases().await {
		Ok(releases) => {
			// Fetching latest releases
			for release in releases {
				if !release.prerelease {
					return Ok(release.tag_name);
				}
			}
			// It should never reach this point, but in case we download a default version of polkadot
			Ok(STABLE_VERSION.to_string())
		},
		// If an error with GitHub API return the STABLE_VERSION
		Err(_) => Ok(STABLE_VERSION.to_string()),
	}
}

fn archive_name_by_target() -> Result<String, Error> {
	match OS {
		"macos" => Ok(format!("{}-mac-universal.tar.gz", BIN_NAME)),
		"linux" => Ok(format!("{}-linux.tar.gz", BIN_NAME)),
		_ => Err(Error::UnsupportedPlatform { os: OS }),
	}
}
fn release_folder_by_target() -> Result<&'static str, Error> {
	match OS {
		"macos" => Ok("artifacts/substrate-contracts-node-mac"),
		"linux" => Ok("artifacts/substrate-contracts-node-linux"),
		_ => Err(Error::UnsupportedPlatform { os: OS }),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::Result;

	#[tokio::test]
	async fn test_latest_polkadot_release() -> Result<()> {
		let version = latest_contract_node_release().await?;
		// Result will change all the time to the current version, check at least starts with v
		assert!(version.starts_with("v"));
		Ok(())
	}
	#[tokio::test]
	async fn release_folder_by_target_works() -> Result<()> {
		let path = release_folder_by_target();
		if cfg!(target_os = "macos") {
			assert_eq!(path?, "artifacts/substrate-contracts-node-mac");
		} else if cfg!(target_os = "linux") {
			assert_eq!(path?, "artifacts/substrate-contracts-node-linux");
		} else {
			assert!(path.is_err())
		}
		Ok(())
	}
	#[tokio::test]
	async fn folder_path_by_target() -> Result<()> {
		let archive = archive_name_by_target();
		if cfg!(target_os = "macos") {
			assert_eq!(archive?, "substrate-contracts-node-mac-universal.tar.gz");
		} else if cfg!(target_os = "linux") {
			assert_eq!(archive?, "substrate-contracts-node-linux.tar.gz");
		} else {
			assert!(archive.is_err())
		}
		Ok(())
	}
}
