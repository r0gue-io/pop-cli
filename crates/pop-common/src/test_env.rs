// SPDX-License-Identifier: GPL-3.0

use crate::{
	Error,
	polkadot_sdk::sort_by_latest_semantic_version,
	resolve_port, set_executable_permission,
	sourcing::{ArchiveFileSpec, Binary, GitHub::ReleaseArchive, Source::GitHub},
};

use serde_json::json;
use std::{
	env::consts::{ARCH, OS},
	process::{Child, Command, Stdio},
	time::Duration,
};
use tokio::time::sleep;

/// Represents a temporary test node process, running locally for testing.
pub struct TestNode {
	child: Child,
	ws_url: String,
	// Needed to be kept alive to avoid deleting the temporaory directory.
	_temp_dir: tempfile::TempDir,
}

impl Drop for TestNode {
	fn drop(&mut self) {
		let _ = self.child.kill();
	}
}

impl TestNode {
	async fn wait_for_node_availability(host: &str, port: u16) -> anyhow::Result<()> {
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

	/// Spawns a local ink! node and waits until it's ready.
	pub async fn spawn() -> anyhow::Result<Self> {
		let temp_dir = tempfile::tempdir()?;
		let random_port = resolve_port(None);
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
				archive: archive_name_by_target()?,
				contents: release_directory_by_target("ink-node")?,
				latest: None,
			})
			.into(),
			cache: cache.to_path_buf(),
		};
		binary.source(false, &(), true).await?;
		set_executable_permission(binary.path())?;

		let mut command = Command::new(binary.path());
		command.arg("--dev");
		command.arg(format!("--rpc-port={}", random_port));
		command.stderr(Stdio::null());
		command.stdout(Stdio::null());

		let child = command.spawn()?;
		let host = "127.0.0.1";

		// Wait until the node is ready
		Self::wait_for_node_availability(host, random_port).await?;

		let ws_url = format!("ws://{host}:{random_port}");

		Ok(Self { child, ws_url, _temp_dir: temp_dir })
	}

	/// Returns the WebSocket URL of the running test node.
	pub fn ws_url(&self) -> &str {
		&self.ws_url
	}
}

fn archive_name_by_target() -> Result<String, Error> {
	match OS {
		"macos" => Ok("ink-node-mac-universal.tar.gz".to_string()),
		"linux" => Ok("ink-node-linux.tar.gz".to_string()),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
}

fn release_directory_by_target(binary: &str) -> Result<Vec<ArchiveFileSpec>, Error> {
	match OS {
		"macos" => Ok("ink-node-mac/ink-node"),
		"linux" => Ok("ink-node-linux/ink-node"),
		_ => Err(Error::UnsupportedPlatform { arch: ARCH, os: OS }),
	}
	.map(|name| vec![ArchiveFileSpec::new(name.into(), Some(binary.into()), true)])
}
