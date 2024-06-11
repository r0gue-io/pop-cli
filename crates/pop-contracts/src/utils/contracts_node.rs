use duct::cmd;
use flate2::read::GzDecoder;
use std::{
	env::consts::OS,
	io::{Seek, SeekFrom, Write},
	path::PathBuf,
};
use tar::Archive;
use tempfile::tempfile;

use crate::errors::Error;

const SUBSTRATE_CONTRACT_NODE: &str = "https://github.com/paritytech/substrate-contracts-node";
const BIN_NAME: &str = "substrate-contracts-node";
const STABLE_VERSION: &str = "v0.41.0";

pub async fn run_contracts_node(cache: PathBuf) -> Result<(), Error> {
	let cached_file = cache.join(folder_name_by_target()?).join(BIN_NAME);
	if !cached_file.exists() {
		let archive = archive_name_by_target()?;
		let releases_url =
			format!("{SUBSTRATE_CONTRACT_NODE}/releases/download/{STABLE_VERSION}/{archive}");
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

fn archive_name_by_target() -> Result<String, Error> {
	match OS {
		"macos" => Ok(format!("{}-mac-universal.tar.gz", BIN_NAME)),
		"linux" => Ok(format!("{}-linux.tar.gz", BIN_NAME)),
		_ => Err(Error::UnsupportedPlatform { os: OS }),
	}
}
fn folder_name_by_target() -> Result<&'static str, Error> {
	match OS {
		"macos" => Ok("artifacts/substrate-contracts-node-mac"),
		"linux" => Ok("artifacts/substrate-contracts-node-linux"),
		_ => Err(Error::UnsupportedPlatform { os: OS }),
	}
}

// #[cfg(test)]
// mod tests {
// 	use super::*;
// 	use anyhow::{Error, Result};

// 	#[tokio::test]
// 	async fn run_contracts_node_works() -> Result<(), Error> {
// 		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
// 		let cache = temp_dir.path().join("cache");
// 		let mut cmd = run_contracts_node(cache.clone()).await?;
// 		// If after 10 secs is still running probably execution is ok, or waiting for user response
// 		sleep(Duration::from_secs(10)).await;

// 		assert!(cmd.try_wait().unwrap().is_none(), "the process should still be running");
// 		// Stop the process
// 		Command::new("kill").args(["-s", "TERM", &cmd.id().to_string()]).spawn()?;
// 		assert!(cache.join(folder_name_by_target()?).join(BIN_NAME).exists());
// 		Ok(())
// 	}
// }
