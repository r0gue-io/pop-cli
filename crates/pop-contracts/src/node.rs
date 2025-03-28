// SPDX-License-Identifier: GPL-3.0

#[cfg(feature = "v6")]
use crate::utils::map_account::MapAccount;

#[cfg(feature = "v5")]
use contract_extrinsics::{RawParams, RpcRequest};
#[cfg(feature = "v6")]
use contract_extrinsics_inkv6::{RawParams, RpcRequest};
use pop_common::{
	sourcing::{
		traits::{Source as _, *},
		Binary,
		GitHub::ReleaseArchive,
		Source,
	},
	Error, GitHub,
};
use strum_macros::{EnumProperty, VariantArray};

use std::{
	env::consts::{ARCH, OS},
	fs::File,
	path::PathBuf,
	process::{Child, Command, Stdio},
	time::Duration,
};
use subxt::{dynamic::Value, SubstrateConfig};
use tokio::time::sleep;

#[cfg(feature = "v5")]
const BIN_NAME: &str = "substrate-contracts-node";
#[cfg(feature = "v6")]
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
	/// Minimal Substrate node configured for smart contracts via pallet-contracts.
	#[strum(props(
		Repository = "https://github.com/paritytech/substrate-contracts-node",
		Binary = "substrate-contracts-node",
		TagFormat = "{tag}",
		Fallback = "v0.41.0"
	))]
	#[cfg(feature = "v5")]
	ContractsNode,
	/// Minimal ink node configured for smart contracts via pallet-revive.
	#[strum(props(
		Repository = "https://github.com/use-ink/ink-node",
		Binary = "ink-node",
		TagFormat = "{tag}",
		Fallback = "v0.43.0"
	))]
	#[cfg(feature = "v6")]
	ContractsNode,
}

#[cfg(any(feature = "v5", feature = "v6"))]
impl TryInto for Chain {
	/// Attempt the conversion.
	///
	/// # Arguments
	/// * `tag` - If applicable, a tag used to determine a specific release.
	/// * `latest` - If applicable, some specifier used to determine the latest source.
	fn try_into(&self, tag: Option<String>, latest: Option<String>) -> Result<Source, Error> {
		let archive = archive_name_by_target()?;
		let archive_bin_path = release_directory_by_target(
			#[cfg(feature = "v5")]
			tag.as_deref(),
		)?;
		Ok(match self {
			&Chain::ContractsNode => {
				// Source from GitHub release asset
				let repo = GitHub::parse(self.repository())?;
				Source::GitHub(ReleaseArchive {
					owner: repo.org,
					repository: repo.name,
					tag,
					tag_format: self.tag_format().map(|t| t.into()),
					archive,
					contents: vec![(archive_bin_path, Some(self.binary().to_string()))],
					latest,
				})
			},
		})
	}
}

impl pop_common::sourcing::traits::Source for Chain {}

/// Retrieves the latest release of the contracts node binary, resolves its version, and constructs
/// a `Binary::Source` with the specified cache path.
///
/// # Arguments
/// * `cache` -  The cache directory path.
/// * `version` - The specific version used for the substrate-contracts-node (`None` will use the
///   latest available version).
pub async fn contracts_node_generator(
	cache: PathBuf,
	version: Option<&str>,
) -> Result<Binary, Error> {
	let chain = &Chain::ContractsNode;
	let name = chain.binary();
	let releases = chain.releases().await?;
	let tag = Binary::resolve_version(name, version, &releases, &cache);
	let latest = version.is_none().then(|| releases.first().map(|v| v.to_string())).flatten();
	let contracts_node = Binary::Source {
		name: name.to_string(),
		#[cfg(any(feature = "v5", feature = "v6"))]
		source: TryInto::try_into(chain, tag.clone(), latest)?,
		cache: cache.to_path_buf(),
	};
	Ok(contracts_node)
}

/// Runs the latest version of the `substrate-contracts-node` in the background.
///
/// # Arguments
///
/// * `binary_path` - The path where the binary is stored. Can be the binary name itself if in PATH.
/// * `output` - The optional log file for node output.
/// * `port` - The WebSocket port on which the node will listen for connections.
pub async fn run_contracts_node(
	binary_path: PathBuf,
	output: Option<&File>,
	port: u16,
) -> Result<Child, Error> {
	let mut command = Command::new(binary_path);
	command.arg("-linfo,runtime::contracts=debug");
	command.arg(format!("--rpc-port={}", port));
	if let Some(output) = output {
		command.stdout(Stdio::from(output.try_clone()?));
		command.stderr(Stdio::from(output.try_clone()?));
	}

	let process = command.spawn()?;

	// Wait until the node is ready
	sleep(STARTUP).await;

	#[cfg(feature = "v5")]
	let data = Value::from_bytes(subxt::utils::to_hex("initialize contracts node"));
	#[cfg(feature = "v5")]
	let payload = subxt::dynamic::tx("System", "remark", [data].to_vec());
	#[cfg(feature = "v6")]
	let payload = MapAccount::new().build();

	let client = subxt::client::OnlineClient::<SubstrateConfig>::from_url(format!(
		"ws://127.0.0.1:{}",
		port
	))
	.await
	.map_err(|e| Error::AnyhowError(e.into()))?;
	client
		.tx()
		.sign_and_submit_default(&payload, &subxt_signer::sr25519::dev::alice())
		.await
		.map_err(|e| Error::AnyhowError(e.into()))?;

	Ok(process)
}

fn archive_name_by_target() -> Result<String, Error> {
	match OS {
		"macos" => Ok(format!("{}-mac-universal.tar.gz", BIN_NAME)),
		"linux" => Ok(format!("{}-linux.tar.gz", BIN_NAME)),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
}
#[cfg(feature = "v6")]
fn release_directory_by_target() -> Result<&'static str, Error> {
	match OS {
		"macos" => Ok("ink-node-mac/ink-node"),
		"linux" => Ok("ink-node-linux/ink-node"),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
}

#[cfg(feature = "v5")]
fn release_directory_by_target(tag: Option<&str>) -> Result<&'static str, Error> {
	let is_old_structure = matches!(tag, Some(tag) if tag < "v0.42.0");
	match OS {
		"macos" =>
			if is_old_structure {
				Ok("artifacts/substrate-contracts-node-mac/substrate-contracts-node")
			} else {
				Ok("substrate-contracts-node-mac/substrate-contracts-node")
			},
		"linux" =>
			if is_old_structure {
				Ok("artifacts/substrate-contracts-node-linux/substrate-contracts-node")
			} else {
				Ok("substrate-contracts-node-linux/substrate-contracts-node")
			},
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use anyhow::{Error, Result};
	use pop_common::find_free_port;
	use std::process::Command;

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
		let polkadot_url = url::Url::parse("wss://polkadot-rpc.dwellir.com")?;
		assert!(is_chain_alive(polkadot_url).await?);
		Ok(())
	}

	#[tokio::test]
	async fn contracts_node_generator_works() -> anyhow::Result<()> {
		let expected = Chain::ContractsNode;
		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = temp_dir.path().join("cache");
		let version = "v0.40.0";
		let binary = contracts_node_generator(cache.clone(), Some(version)).await?;
		let archive = archive_name_by_target()?;
		let archive_bin_path = release_directory_by_target(
			#[cfg(feature = "v5")]
			Some(version),
		)?;
		#[cfg(feature = "v5")]
		let owner = "paritytech";
		#[cfg(feature = "v6")]
		let owner = "use-ink";
		assert!(matches!(binary, Binary::Source { name, source, cache}
			if name == expected.binary()  &&
				source == Source::GitHub(ReleaseArchive {
					owner: owner.to_string(),
					repository: BIN_NAME.to_string(),
					tag: Some(version.to_string()),
					tag_format: expected.tag_format().map(|t| t.into()),
					archive: archive,
					contents: vec![(archive_bin_path, Some(binary.name().to_string()))],
					latest: None,
				})
				&&
			cache == cache
		));
		Ok(())
	}

	#[ignore = "Works fine locally but is causing issues when running tests in parallel in the CI environment."]
	#[tokio::test]
	async fn run_contracts_node_works() -> Result<(), Error> {
		let random_port = find_free_port(None);
		let localhost_url = format!("ws://127.0.0.1:{}", random_port);
		let local_url = url::Url::parse(&localhost_url)?;

		let temp_dir = tempfile::tempdir().expect("Could not create temp dir");
		let cache = temp_dir.path().join("");

		let version = "v0.40.0";
		let binary = contracts_node_generator(cache.clone(), Some(version)).await?;
		binary.source(false, &(), true).await?;
		let process = run_contracts_node(binary.path(), None, 9947).await?;

		// Check if the node is alive
		assert!(is_chain_alive(local_url).await?);
		assert!(cache.join("substrate-contracts-node-v0.40.0").exists());
		assert!(!cache.join("artifacts").exists());
		// Stop the process contracts-node
		Command::new("kill")
			.args(["-s", "TERM", &process.id().to_string()])
			.spawn()?
			.wait()?;

		Ok(())
	}
}
