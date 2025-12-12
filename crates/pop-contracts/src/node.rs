// SPDX-License-Identifier: GPL-3.0

use crate::utils::map_account::MapAccount;
use contract_extrinsics::{RawParams, RpcRequest};
use pop_common::{
	Error, GitHub,
	polkadot_sdk::sort_by_latest_semantic_version,
	sourcing::{
		ArchiveType,
		GitHub::ReleaseArchive,
		Source, SourcedArchive,
		traits::{
			Source as SourceT,
			enums::{Source as _, *},
		},
	},
};
use strum_macros::{EnumProperty, VariantArray};

use pop_common::sourcing::{ArchiveFileSpec, filters::prefix};
use std::{
	env::consts::{ARCH, OS},
	fs::File,
	path::{Path, PathBuf},
	process::{Child, Command, Stdio},
	time::Duration,
};
use subxt::{SubstrateConfig, client};
use tokio::time::sleep;

const BIN_NAME: &str = "ink-node";
const STARTUP: Duration = Duration::from_millis(20_000);

/// Checks if the specified node is alive and responsive.
///
/// # Arguments
///
/// * `url` - Endpoint of the node.
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

/// A supported chain.
#[derive(Debug, EnumProperty, PartialEq, VariantArray)]
pub(super) enum Chain {
	/// RPC module for Ethereum compatibility.
	#[strum(props(
		Repository = "https://github.com/use-ink/ink-node",
		Binary = "eth-rpc",
		Fallback = "v0.47.0"
	))]
	EthRpc,
	/// Minimal ink node configured for smart contracts via pallet-revive.
	#[strum(props(
		Repository = "https://github.com/use-ink/ink-node",
		Binary = "ink-node",
		Fallback = "v0.47.0"
	))]
	ContractsNode,
}

impl SourceT for Chain {
	type Error = Error;
	/// Defines the source of a binary for the chain.
	fn source(&self) -> Result<Source, Error> {
		// Source from GitHub release asset
		let repo = GitHub::parse(self.repository())?;
		Ok(Source::GitHub(ReleaseArchive {
			owner: repo.org,
			repository: repo.name,
			tag: None,
			tag_pattern: self.tag_pattern().map(|t| t.into()),
			prerelease: false,
			version_comparator: sort_by_latest_semantic_version,
			fallback: self.fallback().into(),
			archive: archive_name_by_target()?,
			contents: release_directory_by_target()?,
			latest: None,
		}))
	}
}

/// Retrieves the latest release of the contracts node binary, resolves its version, and constructs
/// a `SourcedArchive::Source` with the specified cache path.
///
/// # Arguments
/// * `cache` - The cache directory path.
/// * `version` - The specific version used for the ink-node (`None` will use the latest available
///   version).
pub async fn ink_node_generator(
	cache: PathBuf,
	version: Option<&str>,
) -> Result<SourcedArchive, Error> {
	node_generator_inner(&Chain::ContractsNode, cache, version).await
}

/// Retrieves the latest release of the Ethereum RPC binary, resolves its version, and constructs
/// a `SourcedArchive::Source` with the specified cache path.
///
/// # Arguments
/// * `cache` - The cache directory path.
/// * `version` - The specific version used for the eth-rpc (`None` will use the latest available
///   version).
pub async fn eth_rpc_generator(
	cache: PathBuf,
	version: Option<&str>,
) -> Result<SourcedArchive, Error> {
	node_generator_inner(&Chain::EthRpc, cache, version).await
}

async fn node_generator_inner(
	chain: &Chain,
	cache: PathBuf,
	version: Option<&str>,
) -> Result<SourcedArchive, Error> {
	let name = chain.binary()?.to_string();
	let source = chain
		.source()?
		.resolve(&name, version, &cache, |f| prefix(f, &name))
		.await
		.into();
	Ok(SourcedArchive::Source { name, source, cache, archive_type: ArchiveType::Binary })
}

/// Runs the latest version of the `ink-node` in the background.
///
/// # Arguments
///
/// * `binary_path` - The path where the binary is stored. Can be the binary name itself if in PATH.
/// * `output` - The optional log file for node output.
/// * `port` - The WebSocket port on which the node will listen for connections.
pub async fn run_ink_node(
	binary_path: &Path,
	output: Option<&File>,
	port: u16,
) -> Result<Child, Error> {
	let mut command = Command::new(binary_path);
	command.arg("-linfo,runtime::contracts=debug");
	command.arg(format!("--rpc-port={}", port));
	command.arg("--tmp");
	if let Some(output) = output {
		command.stdout(Stdio::from(output.try_clone()?));
		command.stderr(Stdio::from(output.try_clone()?));
	}

	let process = command.spawn()?;

	// Wait until the node is ready
	sleep(STARTUP).await;

	let payload = MapAccount::new().build();

	let client =
		client::OnlineClient::<SubstrateConfig>::from_url(format!("ws://127.0.0.1:{}", port))
			.await
			.map_err(|e| Error::AnyhowError(e.into()))?;
	client
		.tx()
		.sign_and_submit_default(&payload, &subxt_signer::sr25519::dev::alice())
		.await
		.map_err(|e| Error::AnyhowError(e.into()))?;

	Ok(process)
}

/// Runs the latest version of the `eth_rpc` in the background.
///
/// # Arguments
///
/// * `binary_path` - The path where the binary is stored. Can be the binary name itself if in PATH.
/// * `output` - The optional log file for node output.
/// * `port` - The WebSocket port on which the node will listen for connections.
pub async fn run_eth_rpc_node(
	binary_path: &Path,
	output: Option<&File>,
	node_url: &str,
	port: u16,
) -> Result<Child, Error> {
	let mut command = Command::new(binary_path);
	command.arg(format!("--node-rpc-url={}", node_url));
	command.arg(format!("--rpc-port={}", port));
	if let Some(output) = output {
		command.stdout(Stdio::from(output.try_clone()?));
		command.stderr(Stdio::from(output.try_clone()?));
	}
	Ok(command.spawn()?)
}

fn archive_name_by_target() -> Result<String, Error> {
	match OS {
		"macos" => Ok(format!("{}-mac-universal.tar.gz", BIN_NAME)),
		"linux" => Ok(format!("{}-linux.tar.gz", BIN_NAME)),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
}
fn release_directory_by_target() -> Result<Vec<ArchiveFileSpec>, Error> {
	match OS {
		"macos" => Ok(vec!["ink-node-mac/ink-node", "ink-node-mac/eth-rpc"]),
		"linux" => Ok(vec!["ink-node-linux/ink-node", "ink-node-linux/eth-rpc"]),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
	.map(|files| {
		files
			.into_iter()
			.map(|name| {
				ArchiveFileSpec::new(
					name.into(),
					Some(name.split("/").last().unwrap().into()),
					true,
				)
			})
			.collect()
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};

	const POLKADOT_NETWORK_URL: &str = "wss://polkadot-rpc.publicnode.com";

	#[tokio::test]
	async fn directory_path_by_target() -> Result<()> {
		let archive = archive_name_by_target();
		if cfg!(target_os = "macos") {
			assert_eq!(archive?, format!("{BIN_NAME}-mac-universal.tar.gz"));
		} else if cfg!(target_os = "linux") {
			assert_eq!(archive?, format!("{BIN_NAME}-linux.tar.gz"));
		} else {
			assert!(archive.is_err())
		}
		Ok(())
	}

	#[tokio::test]
	async fn is_chain_alive_works() -> Result<(), Error> {
		let local_url = url::Url::parse("ws://wrong")?;
		assert!(!is_chain_alive(local_url).await?);
		let polkadot_url = url::Url::parse(POLKADOT_NETWORK_URL)?;
		assert!(is_chain_alive(polkadot_url).await?);
		Ok(())
	}

	#[tokio::test]
	async fn contracts_node_generator_works() -> anyhow::Result<()> {
		let expected = Chain::ContractsNode;
		let archive = archive_name_by_target()?;
		let contents = release_directory_by_target()?;
		let owner = "use-ink";
		let versions = ["v0.43.0"];
		for version in versions {
			let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
			let cache = temp_dir.path().join("cache");
			let binary = ink_node_generator(cache.clone(), Some(version)).await?;

			assert!(matches!(binary, SourcedArchive::Source { name, source, cache, archive_type}
				if name == expected.binary().unwrap() &&
					*source == Source::GitHub(ReleaseArchive {
						owner: owner.to_string(),
						repository: BIN_NAME.to_string(),
							tag: Some(version.to_string()),
							tag_pattern: expected.tag_pattern().map(|t| t.into()),
							prerelease: false,
							version_comparator: sort_by_latest_semantic_version,
							fallback: expected.fallback().into(),
							archive: archive.clone(),
							contents: contents.clone(),
							latest: None,
						})
					&&
				cache == cache && archive_type == ArchiveType::Binary
			));
		}
		Ok(())
	}
}
