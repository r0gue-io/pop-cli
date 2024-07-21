use crate::errors::Error;
use contract_extrinsics::{RawParams, RpcRequest};
use duct::cmd;
use pop_parachains::{Binary, GitHubSource, Source};
use std::{
	env::consts::OS,
	fs::File,
	path::PathBuf,
	process::{Child, Command, Stdio},
	time::Duration,
};
use tokio::time::sleep;

const BIN_NAME: &str = "substrate-contracts-node";

/// Checks if the specified node is alive and responsive.
///
/// # Arguments
///
/// * `url` - Endpoint of the node.
///
pub async fn is_chain_alive(url: url::Url) -> Result<bool, Error> {
	let request = RpcRequest::new(&url).await;
	match request {
		Ok(request) => {
			let params = RawParams::new(&[])?;
			let result = request.raw_call("system_health", params).await;
			match result {
				Ok(_) => Ok(true),
				Err(_) => Ok(false),
			}
		},
		Err(_) => Ok(false),
	}
}

/// Checks if the `substrate-contracts-node` binary exists
/// or if the binary exists in pop's cache.
/// returns:
/// - Some("", <version-output>) if the standalone binary exists
/// - Some(binary_cache_location, "") if the binary exists in pop's cache
/// - None if the binary does not exist
pub fn does_contracts_node_exist(cache: PathBuf) -> Option<(PathBuf, String)> {
	let cached_location = cache.join(BIN_NAME);
	let standalone_output = cmd(BIN_NAME, vec!["--version"]).read();

	if standalone_output.is_ok() {
		Some((PathBuf::new(), standalone_output.unwrap()))
	} else if cached_location.exists() {
		Some((cached_location, "".to_string()))
	} else {
		None
	}
}

/// Downloads the latest version of the `substrate-contracts-node` binary
/// into the specified cache location.
pub async fn download_contracts_node(cache: PathBuf) -> Result<Binary, Error> {
	let archive = archive_name_by_target()?;
	let archive_bin_path = release_folder_by_target()?;

	let source = Source::GitHub(GitHubSource::ReleaseArchive {
		owner: "paritytech".into(),
		repository: "substrate-contracts-node".into(),
		tag: None,
		tag_format: None,
		archive,
		contents: vec![(archive_bin_path, Some(BIN_NAME.to_string()))],
		latest: None,
	});

	let contracts_node =
		Binary::Source { name: "substrate-contracts-node".into(), source, cache: cache.clone() };

	// source the substrate-contracts-node binary
	contracts_node
		.source(false, &(), true)
		.await
		.map_err(|err| Error::SourcingError(err))?;

	Ok(contracts_node)
}

/// Runs the latest version of the `substrate-contracts-node` in the background.
///
/// # Arguments
///
/// * `binary_path` - The path where the binary is stored. Can be the binary name itself if in PATH.
/// * `output` - The optional log file for node output.
///
pub async fn run_contracts_node(
	binary_path: PathBuf,
	output: Option<&File>,
) -> Result<Child, Error> {
	let mut command = Command::new(binary_path);

	if let Some(output) = output {
		command.stdout(Stdio::from(output.try_clone()?));
		command.stderr(Stdio::from(output.try_clone()?));
	}

	let process = command.spawn()?;

	// Wait 5 secs until the node is ready
	sleep(Duration::from_millis(5000)).await;
	Ok(process)
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
		"macos" => Ok("artifacts/substrate-contracts-node-mac/substrate-contracts-node"),
		"linux" => Ok("artifacts/substrate-contracts-node-linux/substrate-contracts-node"),
		_ => Err(Error::UnsupportedPlatform { os: OS }),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use std::process::Command;

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

	#[tokio::test]
	async fn is_chain_alive_works() -> Result<(), Error> {
		let local_url = url::Url::parse("ws://wrong")?;
		assert!(!is_chain_alive(local_url).await?);
		let polkadot_url = url::Url::parse("wss://polkadot-rpc.dwellir.com")?;
		assert!(is_chain_alive(polkadot_url).await?);
		Ok(())
	}

	#[tokio::test]
	async fn run_contracts_node_works() -> Result<(), Error> {
		let local_url = url::Url::parse("ws://localhost:9944")?;
		// Run the contracts node
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = temp_dir.path().join("cache");
		let process = run_contracts_node(cache.clone(), None).await?;
		// Check if the node is alive
		assert!(is_chain_alive(local_url).await?);
		assert!(cache.join("substrate-contracts-node").exists());
		assert!(!cache.join("artifacts").exists());
		// Stop the process contracts-node
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;
		Ok(())
	}
}
