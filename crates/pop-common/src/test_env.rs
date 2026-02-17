// SPDX-License-Identifier: GPL-3.0

use crate::{
	Error,
	polkadot_sdk::{sort_by_latest_semantic_version, sort_by_latest_stable_version},
	resolve_port, set_executable_permission,
	sourcing::{ArchiveFileSpec, Binary, GitHub::ReleaseArchive, Source::GitHub},
	target,
};

use serde_json::json;
use std::{
	env::consts::{ARCH, OS},
	process::{Child, Command, Stdio},
	time::Duration,
};
use tokio::time::sleep;

/// Internal struct representing a running test node process.
struct NodeProcess {
	child: Child,
	ws_url: String,
	_temp_dir: tempfile::TempDir,
	_cache_dir: Option<tempfile::TempDir>,
}

impl Drop for NodeProcess {
	fn drop(&mut self) {
		let _ = self.child.kill();
	}
}

impl NodeProcess {
	/// Wait for the node to become available via RPC.
	async fn wait_for_availability(host: &str, port: u16) -> anyhow::Result<()> {
		let mut attempts = 0;
		let url = format!("http://{host}:{port}");
		let client = reqwest::Client::new();
		let payload = json!({
			"jsonrpc": "2.0",
			"id": 1,
			"method": "system_health",
			"params": []
		});

		loop {
			sleep(Duration::from_secs(2)).await;
			match client.post(&url).json(&payload).send().await {
				Ok(resp) => {
					let text = resp.text().await?;
					if !text.is_empty() {
						return Ok(());
					}
				},
				Err(_) => {
					attempts += 1;
					if attempts > 10 {
						return Err(anyhow::anyhow!("Node could not be started"));
					}
				},
			}
		}
	}

	/// Spawn a node with the given binary configuration.
	///
	/// If the binary is extracted to a temporary cache directory, pass it as `cache_dir`
	/// to ensure it remains alive for the lifetime of the node process.
	async fn spawn(binary: Binary, cache_dir: Option<tempfile::TempDir>) -> anyhow::Result<Self> {
		let temp_dir = tempfile::tempdir()?;
		let random_port = resolve_port(None);

		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;

		let mut command = Command::new(binary.path());
		command.arg("--dev");
		command.arg(format!("--rpc-port={}", random_port));
		command.stderr(Stdio::null());
		command.stdout(Stdio::null());

		let child = command.spawn()?;
		let host = "127.0.0.1";

		Self::wait_for_availability(host, random_port).await?;

		let ws_url = format!("ws://{host}:{random_port}");

		Ok(Self { child, ws_url, _temp_dir: temp_dir, _cache_dir: cache_dir })
	}

	fn ws_url(&self) -> &str {
		&self.ws_url
	}
}

/// Represents a temporary ink! test node process for contract testing.
///
/// This node includes pallet-revive for smart contract functionality.
/// For non-contract testing (chain calls, storage, metadata), use `SubstrateTestNode` instead.
pub struct InkTestNode(NodeProcess);

impl InkTestNode {
	/// Spawns a local ink! node and waits until it's ready.
	pub async fn spawn() -> anyhow::Result<Self> {
		let temp_dir = tempfile::tempdir()?;
		let cache = temp_dir.path().to_path_buf();

		let binary = Binary::Source {
			name: "ink-node".to_string(),
			source: GitHub(ReleaseArchive {
				owner: "use-ink".into(),
				repository: "ink-node".into(),
				tag: None,
				tag_pattern: Some("v{version}".into()),
				prerelease: false,
				version_comparator: sort_by_latest_semantic_version,
				fallback: "v0.47.0".to_string(),
				archive: ink_node_archive()?,
				contents: ink_node_contents()?,
				latest: None,
			})
			.into(),
			cache: cache.to_path_buf(),
		};

		NodeProcess::spawn(binary, Some(temp_dir)).await.map(Self)
	}

	/// Returns the WebSocket URL of the running test node.
	pub fn ws_url(&self) -> &str {
		self.0.ws_url()
	}
}

/// Represents a temporary Substrate test node process for testing chain functionality.
///
/// This node is intended for testing non-contract features like:
/// - Chain calls and extrinsics
/// - Storage queries
/// - Metadata parsing
/// - Runtime operations
///
/// For contract-specific testing, use `InkTestNode` which runs ink-node with pallet-revive.
pub struct SubstrateTestNode(NodeProcess);

impl SubstrateTestNode {
	/// Spawns a local Substrate test node and waits until it's ready.
	///
	/// This uses substrate-node from r0gue-io/polkadot releases, which provides
	/// a minimal Substrate runtime suitable for testing chain functionality.
	pub async fn spawn() -> anyhow::Result<Self> {
		let temp_dir = tempfile::tempdir()?;
		let cache = temp_dir.path().to_path_buf();

		let binary = Binary::Source {
			name: "substrate-node".to_string(),
			source: GitHub(ReleaseArchive {
				owner: "r0gue-io".into(),
				repository: "polkadot".into(),
				tag: Some("polkadot-stable2512-1".into()),
				tag_pattern: Some("polkadot-{version}".into()),
				prerelease: false,
				version_comparator: sort_by_latest_stable_version,
				fallback: "stable2512-1".to_string(),
				archive: format!("substrate-node-{}.tar.gz", target()?),
				contents: vec![ArchiveFileSpec::new("substrate-node".into(), None, true)],
				latest: None,
			})
			.into(),
			cache: cache.to_path_buf(),
		};

		NodeProcess::spawn(binary, Some(temp_dir)).await.map(Self)
	}

	/// Returns the WebSocket URL of the running test node.
	pub fn ws_url(&self) -> &str {
		self.0.ws_url()
	}
}

fn ink_node_archive() -> Result<String, Error> {
	match OS {
		"macos" => Ok("ink-node-mac-universal.tar.gz".to_string()),
		"linux" => Ok("ink-node-linux.tar.gz".to_string()),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
}

fn ink_node_contents() -> Result<Vec<ArchiveFileSpec>, Error> {
	match OS {
		"macos" => Ok("ink-node-mac/ink-node"),
		"linux" => Ok("ink-node-linux/ink-node"),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
	.map(|name| vec![ArchiveFileSpec::new(name.into(), Some("ink-node".into()), true)])
}
