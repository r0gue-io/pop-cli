// SPDX-License-Identifier: GPL-3.0

use crate::{
	find_free_port,
	polkadot_sdk::sort_by_latest_semantic_version,
	set_executable_permission,
	sourcing::{ArchiveFileSpec, Binary, GitHub::ReleaseArchive, Source::GitHub},
	Error,
};

use std::{
	env::consts::{ARCH, OS},
	process::{Child, Command, Stdio},
	time::Duration,
};
use tokio::time::sleep;

const STARTUP: Duration = Duration::from_millis(12_000);

/// Represents a temporary test node process, running locally for testing.
pub struct TestNode {
	child: Child,
	ws_url: String,
}

impl Drop for TestNode {
	fn drop(&mut self) {
		let _ = self.child.kill();
	}
}

impl TestNode {
	/// Spawns a local ink! node and waits until it's ready.
	pub async fn spawn() -> anyhow::Result<Self> {
		let temp_dir = tempfile::tempdir()?;
		let random_port = find_free_port(None);
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
				fallback: "v0.43.0".to_string(),
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

		// Wait until the node is ready
		sleep(STARTUP).await;

		let ws_url = format!("ws://127.0.0.1:{random_port}");

		Ok(Self { child, ws_url })
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
